mod db;
mod import;
mod netex;
mod source;

use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use std::{
    env,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("fares import failed: {error:#}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let mut database =
        PathBuf::from(env::var("FARES_DB_PATH").unwrap_or_else(|_| "./data/fares.sqlite".into()));
    let mut scratch =
        PathBuf::from(env::var("FARES_SCRATCH_PATH").unwrap_or_else(|_| "./scratch/fares".into()));
    let mut source = env::var("FARES_SOURCE").unwrap_or_else(|_| source::DEFAULT_SOURCE.into());
    let mut minimum = env::var("MIN_FREE_BYTES")
        .unwrap_or_else(|_| "536870912".into())
        .parse::<u64>()?;
    let mut check = false;
    let mut backup = None;
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
                    .context("--source requires a URL, ZIP, XML or directory")?
            }
            "--min-free-bytes" => {
                minimum = args
                    .next()
                    .context("--min-free-bytes requires a number")?
                    .parse()?
            }
            "--check" => check = true,
            "--backup" => {
                backup = Some(PathBuf::from(
                    args.next().context("--backup requires a path")?,
                ))
            }
            "--help" | "-h" => {
                println!(
                    "fares [--db PATH] [--source URL|ZIP|XML|DIR] [--scratch DIR] [--min-free-bytes N] [--check | --backup PATH]"
                );
                return Ok(());
            }
            _ => bail!("unknown argument {arg}"),
        }
    }
    ensure!(
        !(check && backup.is_some()),
        "choose only one maintenance operation"
    );
    let parent = database
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
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
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
            eprintln!("another fares importer is running; skipped");
            return Ok(());
        }
        Err(e) => return Err(e).context("acquire importer lock"),
    }
    source::check_disk(parent, minimum)?;
    let mut conn = db::connect(&database)?;
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
    if let Some(path) = backup {
        ensure!(!path.exists(), "backup destination already exists");
        conn.backup(rusqlite::MAIN_DB, &path, None)?;
        println!("backup created: {}", path.display());
        return Ok(());
    }
    for name in ["feed.zip.part", "download.json.part"] {
        match fs::remove_file(scratch.join(name)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    // Dedicated per-database scratch: clean only our reserved work directories
    // left by a killed process while holding the stable database lock.
    for entry in fs::read_dir(&scratch)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("fares-work-")
            && entry.file_type()?.is_dir()
        {
            fs::remove_dir_all(entry.path())?;
        }
    }
    let (path, metadata) = source::download(&source, &scratch, minimum)?;
    let work = tempfile::Builder::new()
        .prefix("fares-work-")
        .tempdir_in(&scratch)?;
    let report = import::import(&mut conn, &path, work.path(), parent, minimum)?;
    let wal_bytes_before_checkpoint = fs::metadata(format!("{}-wal", database.display()))
        .map(|m| m.len())
        .unwrap_or(0);
    if report.sqlite_changes > 0
        && let Err(error) = db::checkpoint(&conn)
    {
        eprintln!("update committed; checkpoint deferred: {error:#}");
    }
    if let Some(metadata) = metadata {
        let save = || -> Result<()> {
            fs::write(
                scratch.join("download.json.part"),
                serde_json::to_vec(&metadata)?,
            )?;
            fs::rename(
                scratch.join("download.json.part"),
                scratch.join("download.json"),
            )?;
            Ok(())
        };
        if let Err(e) = save() {
            eprintln!("update committed; download metadata not saved: {e:#}");
        }
    }
    let memory = fs::read_to_string("/proc/self/status").ok().and_then(|s| {
        s.lines()
            .find(|l| l.starts_with("VmHWM:"))
            .map(str::to_owned)
    });
    println!(
        "{}",
        serde_json::json!({"report":report,"peak_memory":memory,"wal_bytes_before_checkpoint":wal_bytes_before_checkpoint})
    );
    Ok(())
}

#[cfg(test)]
mod tests;
