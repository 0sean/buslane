use anyhow::{Context, Result};
use postgres::Client;

struct ValidationCheck {
    severity: &'static str,
    issue_code: &'static str,
    sql: &'static str,
}

const VALIDATION_CHECKS: &[ValidationCheck] = &[
    ValidationCheck {
        severity: "fatal",
        issue_code: "xml_failure_threshold_exceeded",
        sql: r#"
            SELECT COUNT(*)
            FROM ingest.import_batch
            WHERE id IN (SELECT DISTINCT batch_id FROM fare_staging.load_xml_document)
              AND discovered_xml_count > 0
              AND failed_xml_count::numeric / discovered_xml_count::numeric > xml_failure_threshold
        "#,
    },
    ValidationCheck {
        severity: "fatal",
        issue_code: "duplicate_object_uid",
        sql: r#"
            SELECT COUNT(*)
            FROM (
              SELECT batch_id, object_uid
              FROM fare_staging.load_netex_object
              GROUP BY batch_id, object_uid
              HAVING COUNT(*) > 1
            ) duplicates
        "#,
    },
    ValidationCheck {
        severity: "fatal",
        issue_code: "duplicate_document_scoped_netex_id",
        sql: r#"
            SELECT COUNT(*)
            FROM (
              SELECT batch_id, document_uid, element_name, netex_id, COALESCE(version, '')
              FROM fare_staging.load_netex_object
              GROUP BY batch_id, document_uid, element_name, netex_id, COALESCE(version, '')
              HAVING COUNT(*) > 1
            ) duplicates
        "#,
    },
    ValidationCheck {
        severity: "fatal",
        issue_code: "no_derived_fare_offers",
        sql: "SELECT CASE WHEN COUNT(*) = 0 THEN 1 ELSE 0 END FROM fare_staging.fare_offer",
    },
    ValidationCheck {
        severity: "fatal",
        issue_code: "no_derived_fare_prices",
        sql: "SELECT CASE WHEN COUNT(*) = 0 THEN 1 ELSE 0 END FROM fare_staging.fare_price",
    },
    ValidationCheck {
        severity: "fatal",
        issue_code: "derived_fare_price_without_amount_or_block",
        sql: r#"
            SELECT COUNT(*)
            FROM fare_staging.fare_price
            WHERE amount IS NULL
              AND COALESCE(is_allowed, TRUE) IS TRUE
        "#,
    },
    ValidationCheck {
        severity: "warn",
        issue_code: "missing_uk_publication_delivery_root",
        sql: r#"
            SELECT COUNT(*)
            FROM fare_staging.load_xml_document
            WHERE parse_status = 'parsed'
              AND root_name <> 'PublicationDelivery'
        "#,
    },
    ValidationCheck {
        severity: "warn",
        issue_code: "fare_zone_explicit_stops_without_members",
        sql: r#"
            SELECT COUNT(*)
            FROM (
              SELECT z.batch_id, z.object_id
              FROM netex.fare_zone z
              LEFT JOIN netex.fare_zone_member m
                ON m.batch_id = z.batch_id
               AND m.fare_zone_object_id = z.object_id
              WHERE z.batch_id IN (SELECT DISTINCT batch_id FROM fare_staging.load_xml_document)
                AND COALESCE(z.scoping_method, 'explicitStops') = 'explicitStops'
              GROUP BY z.batch_id, z.object_id
              HAVING COUNT(m.member_ref) = 0
            ) missing_members
        "#,
    },
    ValidationCheck {
        severity: "warn",
        issue_code: "unresolved_internal_reference",
        sql: r#"
            SELECT COUNT(*)
            FROM netex.reference
            WHERE batch_id IN (SELECT DISTINCT batch_id FROM fare_staging.load_xml_document)
              AND resolution_status = 'unresolved'
              AND target_ref !~ '^(noc|naptStop|nptgLocality|fxc):'
        "#,
    },
    ValidationCheck {
        severity: "info",
        issue_code: "out_of_profile_distance_tariff_basis",
        sql: r#"
            SELECT COUNT(*)
            FROM netex.tariff
            WHERE batch_id IN (SELECT DISTINCT batch_id FROM fare_staging.load_xml_document)
              AND tariff_basis = 'distance'
        "#,
    },
];

pub fn run_validation_checks(client: &mut Client, fail_on_warnings: bool) -> Result<()> {
    let batch_id: i64 = client
        .query_one(
            "SELECT DISTINCT batch_id FROM fare_staging.load_xml_document LIMIT 1",
            &[],
        )
        .context("cannot find active fares batch for validation")?
        .get(0);

    for check in VALIDATION_CHECKS {
        let row = client
            .query_one(check.sql, &[])
            .with_context(|| format!("validation query failed: {}", check.issue_code))?;
        let count: i64 = row.get(0);
        if count > 0 {
            client
                .execute(
                    r#"
                    INSERT INTO fare_staging.import_quality_issue (
                      batch_id,
                      severity,
                      issue_code,
                      issue_count,
                      details
                    )
                    VALUES ($1, $2, $3, $4, '{}'::jsonb)
                    "#,
                    &[&batch_id, &check.severity, &check.issue_code, &count],
                )
                .with_context(|| {
                    format!("failed to persist validation issue {}", check.issue_code)
                })?;
        }
    }

    let fatal_count: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM fare_staging.import_quality_issue WHERE severity = 'fatal'",
            &[],
        )
        .context("failed to count fatal validation issues")?
        .get(0);
    let warn_count: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM fare_staging.import_quality_issue WHERE severity = 'warn'",
            &[],
        )
        .context("failed to count warning validation issues")?
        .get(0);

    if fatal_count > 0 {
        anyhow::bail!("fares validation failed with {fatal_count} fatal issue groups");
    }
    if fail_on_warnings && warn_count > 0 {
        anyhow::bail!("fares validation failed with {warn_count} warning issue groups");
    }

    Ok(())
}
