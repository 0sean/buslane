mod db;
mod geometry;
mod import;
mod source;

use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use std::env;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("import failed: {error:#}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let mut database = PathBuf::from(
        env::var("TIMETABLES_DB_PATH").unwrap_or_else(|_| "./data/timetables.sqlite".into()),
    );
    let mut scratch = PathBuf::from(
        env::var("TIMETABLES_SCRATCH_PATH").unwrap_or_else(|_| "./data/scratch".into()),
    );
    let mut source = env::var("GTFS_SOURCE").unwrap_or_else(|_| source::DEFAULT_SOURCE.into());
    let mut minimum = env::var("MIN_FREE_BYTES")
        .unwrap_or_else(|_| "536870912".into())
        .parse::<u64>()?;
    let mut backup = None;
    let mut vacuum = false;
    let mut check = false;
    let mut rebuild_geometry = false;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => database = PathBuf::from(args.next().context("--db requires a path")?),
            "--scratch" => {
                scratch = PathBuf::from(args.next().context("--scratch requires a path")?)
            }
            "--source" => {
                source = args
                    .next()
                    .context("--source requires a URL, ZIP or directory")?
            }
            "--min-free-bytes" => {
                minimum = args
                    .next()
                    .context("--min-free-bytes requires a number")?
                    .parse()?
            }
            "--backup" => {
                backup = Some(PathBuf::from(
                    args.next().context("--backup requires an output path")?,
                ))
            }
            "--vacuum" => vacuum = true,
            "--check" => check = true,
            "--rebuild-geometry" => rebuild_geometry = true,
            "--help" | "-h" => {
                println!(
                    "timetables [--db PATH] [--source URL|ZIP|DIR] [--scratch DIR] [--min-free-bytes N] [--backup PATH | --vacuum | --check | --rebuild-geometry]"
                );
                return Ok(());
            }
            _ => bail!("unknown argument {arg}"),
        }
    }
    let parent = database
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    fs::create_dir_all(parent)?;
    fs::create_dir_all(&scratch)?;
    let mut lock_path = database.as_os_str().to_os_string();
    lock_path.push(".lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(PathBuf::from(lock_path))?;
    match lock.try_lock_exclusive() {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            eprintln!("another importer is running; skipped");
            return Ok(());
        }
        Err(error) => return Err(error).context("acquire stable importer lock"),
    }
    if !check
        && !rebuild_geometry
        && !vacuum
        && backup.is_none()
        && (source.starts_with("https://") || source.starts_with("http://"))
    {
        // A killed downloader cannot run its Drop guard. Clear its reserved
        // partial files under the database lock before checking free space.
        for name in ["feed.zip.part", "download.json.part"] {
            if let Err(error) = fs::remove_file(scratch.join(name))
                && error.kind() != std::io::ErrorKind::NotFound
            {
                return Err(error).context("remove abandoned download");
            }
        }
    }
    source::check_disk(parent, minimum)?;
    let mut conn = db::connect(&database)?;
    ensure!(
        usize::from(backup.is_some())
            + usize::from(vacuum)
            + usize::from(check)
            + usize::from(rebuild_geometry)
            <= 1,
        "choose only one maintenance operation"
    );
    if rebuild_geometry {
        let generation = geometry::rebuild(&mut conn)?;
        db::checkpoint(&conn)?;
        println!("geometry rebuilt at generation {generation}");
        return Ok(());
    }
    if let Some(path) = backup {
        ensure!(!path.exists(), "backup destination already exists");
        conn.backup(rusqlite::MAIN_DB, &path, None)?;
        println!("backup created: {}", path.display());
        return Ok(());
    }
    if vacuum {
        conn.execute_batch("VACUUM")?;
        db::checkpoint(&conn)?;
        return Ok(());
    }
    if check {
        let integrity: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        ensure!(integrity == "ok", "integrity check: {integrity}");
        let foreign: i64 =
            conn.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
                r.get(0)
            })?;
        ensure!(foreign == 0, "foreign key violations: {foreign}");
        println!("database checks passed");
        return Ok(());
    }
    let (path, metadata) = source::download(&source, &scratch, minimum)?;
    let mut input = source::Source::open(&path)?;
    let report = import::import(&mut conn, &mut input, parent, minimum)?;
    if report
        .tables
        .values()
        .any(|t| t.inserted + t.updated + t.deleted > 0)
        && let Err(error) = db::checkpoint(&conn)
    {
        eprintln!("update committed; checkpoint maintenance deferred: {error:#}");
    }
    if let Some(metadata) = metadata {
        let save = || -> Result<()> {
            let partial = scratch.join("download.json.part");
            fs::write(&partial, serde_json::to_vec(&metadata)?)?;
            fs::rename(partial, scratch.join("download.json"))?;
            Ok(())
        };
        if let Err(error) = save() {
            eprintln!("update committed; conditional download metadata was not saved: {error:#}");
        }
    }
    let wal_bytes = fs::metadata(format!("{}-wal", database.display()))
        .map(|m| m.len())
        .unwrap_or(0);
    let memory = fs::read_to_string("/proc/self/status").ok().and_then(|s| {
        s.lines()
            .find(|l| l.starts_with("VmHWM:"))
            .map(str::to_owned)
    });
    let identifier_churn:serde_json::Map<String,serde_json::Value>=report.tables.iter().map(|(table,count)| {
        let current=count.inserted+count.updated+count.unchanged;let previous=count.deleted+count.updated+count.unchanged;
        (table.clone(),serde_json::json!({"new_identity_fraction":if current==0{0.0}else{count.inserted as f64/current as f64},"removed_identity_fraction":if previous==0{0.0}else{count.deleted as f64/previous as f64}}))
    }).collect();
    println!(
        "{}",
        serde_json::json!({"report":report,"wal_bytes":wal_bytes,"peak_memory":memory,"identifier_churn":identifier_churn})
    );
    Ok(())
}

#[cfg(test)]
mod tests;
