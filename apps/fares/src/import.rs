use crate::{db, netex, source};
use anyhow::{Context, Result, ensure};
use flate2::{Compression, write::DeflateEncoder};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    time::Instant,
};
use zip::ZipArchive;

const MAX_XML: u64 = 64 * 1024 * 1024;
const MAX_ZIP: u64 = 8 * 1024 * 1024 * 1024;
const MAX_EXPANDED: u64 = 100 * 1024 * 1024 * 1024;

#[derive(Default, Serialize, Debug)]
pub struct Report {
    pub archives_skipped: u64,
    pub documents_skipped: u64,
    pub documents_inserted: u64,
    pub documents_updated: u64,
    pub documents_deleted: u64,
    pub entities_inserted: u64,
    pub entities_updated: u64,
    pub entities_deleted: u64,
    pub entities_unchanged: u64,
    pub sqlite_changes: u64,
    pub generation: i64,
    pub elapsed_ms: u128,
}
struct Previous {
    id: i64,
    hash: String,
}
struct Importer<'a> {
    conn: &'a Connection,
    scratch: &'a Path,
    database_dir: &'a Path,
    minimum: u64,
    documents: HashMap<String, Previous>,
    archives: HashMap<String, String>,
    archive_aliases: HashMap<String, String>,
    archive_children: HashMap<String, Vec<String>>,
    archive_documents: HashMap<String, Vec<i64>>,
    seen_documents: HashSet<i64>,
    seen_archives: HashSet<String>,
    report: Report,
    expanded: u64,
    entries: u64,
    force: bool,
    root_hash: Option<String>,
}

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn file_hash(path: &Path) -> Result<String> {
    reader_hash(&mut File::open(path)?, u64::MAX)
}
fn reader_hash(reader: &mut impl Read, maximum: u64) -> Result<String> {
    let mut sha = Sha256::new();
    let mut buffer = [0; 262144];
    let mut total = 0u64;
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total = total
            .checked_add(n as u64)
            .context("member length overflow")?;
        ensure!(total <= maximum, "member exceeds size limit");
        sha.update(&buffer[..n]);
    }
    Ok(format!("{:x}", sha.finalize()))
}

pub fn import(
    conn: &mut Connection,
    path: &Path,
    scratch: &Path,
    database_dir: &Path,
    minimum: u64,
) -> Result<Report> {
    let started = Instant::now();
    let changes = conn.total_changes();
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let version: i64 = tx.query_row(
        "SELECT normalisation_version FROM dataset WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    let generation: i64 = tx.query_row("SELECT generation FROM dataset WHERE id=1", [], |r| {
        r.get(0)
    })?;
    let mut root_hash = None;
    if version == db::NORMALISATION_VERSION
        && generation > 0
        && path.is_file()
        && !path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("xml"))
    {
        let previous: Option<String> = tx
            .query_row("SELECT sha256 FROM archives WHERE path=''", [], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(previous) = previous {
            let digest = file_hash(path)?;
            if digest == previous {
                // The outer checksum proves all descendants unchanged. Count
                // small indexes instead of loading every document's metadata.
                source::check_disk(database_dir, minimum)?;
                let documents_skipped = tx
                    .query_row("SELECT count(*) FROM documents", [], |r| r.get::<_, i64>(0))?
                    as u64;
                let archives_skipped = tx
                    .query_row("SELECT count(*) FROM archives", [], |r| r.get::<_, i64>(0))?
                    as u64;
                tx.commit()?;
                return Ok(Report {
                    documents_skipped,
                    archives_skipped,
                    generation,
                    sqlite_changes: conn.total_changes() - changes,
                    elapsed_ms: started.elapsed().as_millis(),
                    ..Default::default()
                });
            }
            root_hash = Some(digest);
        }
    }
    let mut importer = Importer {
        conn: &tx,
        scratch,
        database_dir,
        minimum,
        documents: HashMap::new(),
        archives: HashMap::new(),
        archive_aliases: HashMap::new(),
        archive_children: HashMap::new(),
        archive_documents: HashMap::new(),
        seen_documents: HashSet::new(),
        seen_archives: HashSet::new(),
        report: Report::default(),
        expanded: 0,
        entries: 0,
        force: version != db::NORMALISATION_VERSION,
        root_hash,
    };
    {
        let mut stmt = tx.prepare("SELECT id,path,sha256,archive_path FROM documents")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?;
        for row in rows {
            let (id, path, hash, archive) = row?;
            if let Some(archive) = archive {
                importer
                    .archive_documents
                    .entry(archive)
                    .or_default()
                    .push(id);
            }
            importer.documents.insert(path, Previous { id, hash });
        }
        let mut stmt = tx.prepare("SELECT path,sha256,parent_path FROM archives")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        for row in rows {
            let (path, hash, parent) = row?;
            if parent.as_deref() == Some("")
                && let Some(stable) = bods_dataset_key(&path)
            {
                ensure!(
                    importer
                        .archive_aliases
                        .insert(stable, path.clone())
                        .is_none(),
                    "ambiguous BODS dataset archives in database"
                );
            }
            if let Some(parent) = parent {
                importer
                    .archive_children
                    .entry(parent)
                    .or_default()
                    .push(path.clone());
            }
            importer.archives.insert(path, hash);
        }
    }
    if path.is_dir() {
        importer.directory(path, path)?;
    } else if path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("xml"))
    {
        let bytes = bounded_read(File::open(path)?, MAX_XML)?;
        importer.document("source.xml", None, &bytes)?;
    } else {
        importer.archive(path, "", None, 0)?;
    }
    ensure!(
        !importer.seen_documents.is_empty(),
        "source contains no NeTEx documents; refusing to erase the dataset"
    );
    for old in importer.documents.values() {
        if !importer.seen_documents.contains(&old.id) {
            importer.report.entities_deleted += tx.query_row(
                "SELECT count(*) FROM entities WHERE document_id=?",
                [old.id],
                |r| r.get::<_, i64>(0),
            )? as u64;
            tx.execute("DELETE FROM documents WHERE id=?", [old.id])?;
            importer.report.documents_deleted += 1;
        }
    }
    for old in importer.archives.keys() {
        if !importer.seen_archives.contains(old) {
            tx.execute("DELETE FROM archives WHERE path=?", [old])?;
        }
    }
    let changed = importer.report.documents_inserted
        + importer.report.documents_updated
        + importer.report.documents_deleted
        > 0;
    if changed || importer.force {
        tx.execute("UPDATE dataset SET generation=generation+1,imported_at=CURRENT_TIMESTAMP,normalisation_version=? WHERE id=1",[db::NORMALISATION_VERSION])?;
    }
    source::check_disk(database_dir, minimum)?;
    db::validate_deferred_foreign_keys(&tx)?;
    importer.report.generation =
        tx.query_row("SELECT generation FROM dataset WHERE id=1", [], |r| {
            r.get(0)
        })?;
    let mut report = importer.report;
    tx.commit()?;
    report.sqlite_changes = conn.total_changes() - changes;
    report.elapsed_ms = started.elapsed().as_millis();
    Ok(report)
}

fn bounded_read(reader: impl Read, maximum: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(maximum + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= maximum,
        "member exceeds size limit of {maximum} bytes"
    );
    Ok(bytes)
}
fn safe_name(name: &str) -> Result<String> {
    ensure!(
        !name.starts_with('/')
            && !name.contains(['\\', '\0'])
            && !name.split('/').any(|s| s == ".." || s == "."),
        "unsafe archive member {name:?}"
    );
    Ok(name.replace('%', "%25").replace('!', "%21"))
}

fn bods_dataset_key(path: &str) -> Option<String> {
    let (folder, name) = path.rsplit_once('/')?;
    let parts: Vec<_> = name.splitn(5, '_').collect();
    if parts.len() != 5 || !name.to_ascii_lowercase().ends_with(".zip") {
        return None;
    }
    if parts[0].is_empty()
        || parts[1].is_empty()
        || !parts[0].bytes().all(|b| b.is_ascii_digit())
        || !parts[1].bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    fn formatted(value: &str, template: &str) -> bool {
        value.len() == template.len()
            && value.bytes().zip(template.bytes()).all(|(v, t)| {
                if t == b'0' {
                    v.is_ascii_digit()
                } else {
                    v == t
                }
            })
    }
    if !formatted(parts[2], "0000-00-00") || !formatted(parts[3], "00-00-00") {
        return None;
    }
    Some(format!("{folder}/{}", parts[0]))
}

impl Importer<'_> {
    fn space(&self) -> Result<()> {
        source::check_disk(self.database_dir, self.minimum)?;
        source::check_disk(self.scratch, self.minimum)
    }
    fn skip_archive(&mut self, path: &str) {
        self.seen_archives.insert(path.into());
        self.report.archives_skipped += 1;
        if let Some(docs) = self.archive_documents.get(path) {
            self.seen_documents.extend(docs);
            self.report.documents_skipped += docs.len() as u64;
        }
        if let Some(children) = self.archive_children.get(path).cloned() {
            for child in children {
                self.skip_archive(&child);
            }
        }
    }
    fn archive(
        &mut self,
        path: &Path,
        key: &str,
        parent: Option<&str>,
        depth: usize,
    ) -> Result<()> {
        ensure!(depth <= 8, "archive nesting exceeds 8");
        ensure!(
            !self.seen_archives.contains(key),
            "duplicate logical archive {key}"
        );
        let digest = if key.is_empty() {
            self.root_hash
                .take()
                .map(Ok)
                .unwrap_or_else(|| file_hash(path))?
        } else {
            file_hash(path)?
        };
        if !self.force && self.archives.get(key) == Some(&digest) {
            self.skip_archive(key);
            return Ok(());
        }
        self.space()?;
        let mut zip =
            ZipArchive::new(File::open(path)?).with_context(|| format!("open ZIP {key}"))?;
        let mut names = HashSet::new();
        for i in 0..zip.len() {
            let mut member = zip.by_index(i)?;
            let name = safe_name(member.name())?;
            ensure!(names.insert(name.clone()), "duplicate ZIP member {name}");
            if member.is_dir() {
                continue;
            }
            self.entries += 1;
            ensure!(self.entries <= 1_000_000, "too many archive members");
            let mut member_key = if key.is_empty() {
                name.clone()
            } else {
                format!("{key}!/{name}")
            };
            let lower = name.to_ascii_lowercase();
            if key.is_empty()
                && lower.ends_with(".zip")
                && let Some(stable) = bods_dataset_key(&member_key)
            {
                // Keep the first observed archive path as its logical namespace
                // when BODS changes only revision/timestamp components.
                if let Some(previous) = self.archive_aliases.get(&stable) {
                    member_key = previous.clone();
                } else {
                    self.archive_aliases.insert(stable, member_key.clone());
                }
            }
            if !lower.ends_with(".xml") && !lower.ends_with(".zip") {
                continue;
            }
            let maximum = if lower.ends_with(".xml") {
                MAX_XML
            } else {
                MAX_ZIP
            };
            ensure!(
                member.size() <= maximum,
                "member {member_key} exceeds size limit"
            );
            self.expanded = self
                .expanded
                .checked_add(member.size())
                .context("expanded archive size overflow")?;
            ensure!(
                self.expanded <= MAX_EXPANDED,
                "archive expanded size exceeds 100 GiB"
            );
            if lower.ends_with(".xml") {
                let bytes = bounded_read(&mut member, MAX_XML)?;
                self.document(&member_key, Some(key), &bytes)
                    .with_context(|| format!("import {member_key}"))?;
            } else {
                if !self.force
                    && let Some(previous) = self.archives.get(&member_key)
                {
                    ensure!(
                        !self.seen_archives.contains(&member_key),
                        "duplicate logical archive {member_key}"
                    );
                    if reader_hash(&mut member, MAX_ZIP)? == *previous {
                        // Daily bulk archives are repackaged even when most
                        // datasets did not change. Hash those bytes in place.
                        self.skip_archive(&member_key);
                        continue;
                    }
                    drop(member);
                    member = zip.by_index(i)?;
                }
                // Never extract archive-provided paths. A single temporary file
                // per nesting level bounds both memory and scratch usage.
                let mut temp = tempfile::NamedTempFile::new_in(self.scratch)?;
                let mut total = 0;
                let mut buffer = [0; 262144];
                loop {
                    let n = member.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    total += n as u64;
                    ensure!(total <= MAX_ZIP, "nested archive exceeds size limit");
                    temp.write_all(&buffer[..n])?;
                    if total % (8 * 1024 * 1024) < n as u64 {
                        self.space()?;
                    }
                }
                temp.flush()?;
                self.archive(temp.path(), &member_key, Some(key), depth + 1)?;
            }
        }
        self.conn.execute("INSERT INTO archives(path,sha256,parent_path) VALUES(?,?,?) ON CONFLICT(path) DO UPDATE SET sha256=excluded.sha256,parent_path=excluded.parent_path WHERE sha256<>excluded.sha256 OR parent_path IS NOT excluded.parent_path",params![key,digest,parent])?;
        self.seen_archives.insert(key.into());
        Ok(())
    }
    fn directory(&mut self, root: &Path, path: &Path) -> Result<()> {
        let mut children = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        children.sort_by_key(|e| e.file_name());
        for entry in children {
            let ty = entry.file_type()?;
            ensure!(!ty.is_symlink(), "symlink in source directory");
            if ty.is_dir() {
                self.directory(root, &entry.path())?;
                continue;
            }
            let key = safe_name(
                entry
                    .path()
                    .strip_prefix(root)?
                    .to_str()
                    .context("non UTF-8 source path")?,
            )?;
            let lower = key.to_ascii_lowercase();
            if lower.ends_with(".zip") {
                self.archive(&entry.path(), &key, None, 0)?;
            } else if lower.ends_with(".xml") {
                let bytes = bounded_read(File::open(entry.path())?, MAX_XML)?;
                self.document(&key, None, &bytes)
                    .with_context(|| format!("import {key}"))?;
            }
        }
        Ok(())
    }
    fn document(&mut self, path: &str, archive: Option<&str>, bytes: &[u8]) -> Result<()> {
        let digest = hash(bytes);
        if !self.force
            && let Some(old) = self.documents.get(path)
            && old.hash == digest
        {
            self.seen_documents.insert(old.id);
            self.report.documents_skipped += 1;
            return Ok(());
        }
        self.space()?;
        let parsed = netex::parse(bytes)?;
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(bytes)?;
        let compressed = encoder.finish()?;
        let existing = self.documents.get(path).map(|d| d.id);
        let id = if let Some(id) = existing {
            self.conn.execute("UPDATE documents SET archive_path=?,sha256=?,xml_deflate=?,publication_timestamp=?,currency=?,updated_at=CURRENT_TIMESTAMP WHERE id=?",params![archive,digest,compressed,parsed.timestamp,parsed.currency,id])?;
            self.report.documents_updated += 1;
            id
        } else {
            self.conn.execute("INSERT INTO documents(path,archive_path,sha256,xml_deflate,publication_timestamp,currency) VALUES(?,?,?,?,?,?)",params![path,archive,digest,compressed,parsed.timestamp,parsed.currency])?;
            self.report.documents_inserted += 1;
            self.conn.last_insert_rowid()
        };
        self.seen_documents.insert(id);
        let mut old = HashMap::<String, (i64, String)>::new();
        if existing.is_some() {
            let mut stmt = self
                .conn
                .prepare_cached("SELECT identity,id,sha256 FROM entities WHERE document_id=?")?;
            let rows = stmt.query_map([id], |r| {
                Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?)))
            })?;
            for row in rows {
                let (key, value) = row?;
                old.insert(key, value);
            }
        }
        for entity in parsed.entities {
            let sha = hash(&serde_json::to_vec(&entity)?);
            let previous = old.remove(&entity.identity);
            if previous.as_ref().is_some_and(|(_, hash)| hash == &sha) {
                self.report.entities_unchanged += 1;
                continue;
            }
            // Do not use INSERT .. ON CONFLICT on unchanged records: even an
            // ignored AUTOINCREMENT insert would write sqlite_sequence.
            let values = params![
                id,
                entity.identity,
                entity.category,
                entity.kind,
                entity.reference,
                entity.version,
                entity.occurrence,
                entity.name,
                entity.amount,
                entity.currency,
                entity.product_ref,
                entity.profile_ref,
                entity.package_ref,
                entity.matrix_ref,
                entity.price_ref,
                entity.group_ref,
                entity.start_ref,
                entity.end_ref,
                entity.data,
                sha
            ];
            if let Some((row_id, _)) = previous {
                self.conn.prepare_cached("UPDATE entities SET document_id=?1,identity=?2,category=?3,kind=?4,ref=?5,version=?6,occurrence=?7,name=?8,amount=?9,currency=?10,product_ref=?11,profile_ref=?12,package_ref=?13,matrix_ref=?14,price_ref=?15,group_ref=?16,start_ref=?17,end_ref=?18,data=?19,sha256=?20 WHERE document_id=?1 AND identity=?2")?.execute(values)?;
                let _ = row_id;
                self.report.entities_updated += 1;
            } else {
                self.conn.prepare_cached("INSERT INTO entities(document_id,identity,category,kind,ref,version,occurrence,name,amount,currency,product_ref,profile_ref,package_ref,matrix_ref,price_ref,group_ref,start_ref,end_ref,data,sha256) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)")?.execute(values)?;
                self.report.entities_inserted += 1;
            }
        }
        for (_, (entity_id, _)) in old {
            self.conn
                .prepare_cached("DELETE FROM entities WHERE id=?")?
                .execute([entity_id])?;
            self.report.entities_deleted += 1;
        }
        let mut old_links = BTreeMap::new();
        if existing.is_some() {
            let mut stmt = self.conn.prepare_cached(
                "SELECT kind,code,ref,qualifier,name FROM links WHERE document_id=?",
            )?;
            for row in stmt.query_map([id], |r| {
                Ok((
                    (
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ),
                    r.get::<_, Option<String>>(4)?,
                ))
            })? {
                let (key, name) = row?;
                old_links.insert(key, name);
            }
        }
        for link in parsed.links {
            let key = (
                link.kind.clone(),
                link.code.clone(),
                link.reference.clone(),
                link.qualifier.clone(),
            );
            if old_links.remove(&key).is_some_and(|name| name == link.name) {
                continue;
            }
            self.conn.prepare_cached("INSERT INTO links(document_id,kind,code,ref,qualifier,name) VALUES(?,?,?,?,?,?) ON CONFLICT(document_id,kind,code,ref,qualifier) DO UPDATE SET name=excluded.name WHERE name IS NOT excluded.name")?.execute(params![id,link.kind,link.code,link.reference,link.qualifier,link.name])?;
        }
        for ((kind, code, reference, qualifier), _) in old_links {
            self.conn.prepare_cached("DELETE FROM links WHERE document_id=? AND kind=? AND code=? AND ref=? AND qualifier=?")?.execute(params![id,kind,code,reference,qualifier])?;
        }
        if self.report.documents_inserted + self.report.documents_updated > 0
            && (self.report.documents_inserted + self.report.documents_updated).is_multiple_of(1000)
        {
            eprintln!(
                "fares: {} documents processed",
                self.report.documents_inserted + self.report.documents_updated
            );
        }
        Ok(())
    }
}
