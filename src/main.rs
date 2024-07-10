pub mod entry;

use std::fs;
use std::process::{ExitStatus, Stdio};

use chrono::{DateTime, FixedOffset, Local};
use clap::Parser;
use color_eyre::eyre::Result;
use serde::Deserializer;
use tokio::process::Command;
use tokio::task::JoinSet;

use entry::DeserializeUserRepos;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// The maximum number of repositories to update in this run.
    #[arg(short, long, value_name = "NUM", default_value_t = 100)]
    limit: u64,

    /// The users to back up.
    users: Vec<String>,
}

pub struct BackupFile<'a> {
    path: &'a str,
}

impl<'a> BackupFile<'a> {
    pub fn new(path: &'a str) -> Self {
        Self { path }
    }

    pub fn read(&self) -> Result<DateTime<FixedOffset>> {
        Ok(match fs::read(self.path) {
            Ok(bytes) => DateTime::<FixedOffset>::parse_from_rfc3339(std::str::from_utf8(&bytes)?)?,
            Err(_) => DateTime::UNIX_EPOCH.into(),
        })
    }

    pub fn write(&self, time: DateTime<FixedOffset>) -> Result<(), std::io::Error> {
        fs::write(self.path, time.to_rfc3339())
    }
}

pub async fn git_update(repo: String) -> Result<ExitStatus, std::io::Error> {
    let status = Command::new("git")
        .args(["-C", "pull", &repo])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;
    println!("Pulling from repository '{}'", repo);
    if !status.success() {
        println!("Could not pull: cloning from repository '{}'", repo);
        Command::new("gh")
            .args(["repo", "clone", &repo, &repo])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
    } else {
        Ok(status)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let Cli { limit, users } = Cli::parse();
    let limit_string = limit.to_string();

    let backup_file = ".gh_last_backup";

    // read last updated
    let last_updated = match fs::read(backup_file) {
        Ok(bytes) => DateTime::<FixedOffset>::parse_from_rfc3339(std::str::from_utf8(&bytes)?)?,
        Err(_) => DateTime::UNIX_EPOCH.into(),
    };

    // set the new update time before we begin requesting from the server
    let script_start_time: DateTime<FixedOffset> = Local::now().into();

    // initialize futures
    let mut entry_set = JoinSet::new();
    for user in users {
        let output = Command::new("gh")
            .args([
                "repo",
                "ls",
                &user,
                "--limit",
                &limit_string,
                "--json",
                "nameWithOwner",
                "--json",
                "updatedAt",
            ])
            .output();

        entry_set.spawn(output);
    }

    // join futures to get all entries which require updating
    let mut to_update = Vec::new();
    while let Some(output) = entry_set.join_next().await {
        let output = output??.stdout;

        let mut json_de = serde_json::Deserializer::from_slice(&output);
        json_de.deserialize_seq(DeserializeUserRepos::new(&last_updated, &mut to_update))?;
    }

    // update the corresponding entries
    let mut update_set = JoinSet::new();
    for entry in to_update.drain(..) {
        let cmd = git_update(entry.repo);
        update_set.spawn(cmd);
    }

    let mut ok = true;
    while let Some(status) = update_set.join_next().await {
        ok = ok && status??.success();
    }

    // if everything succeeds, write the start time to the backup file
    if ok {
        println!("writing to backup");
        fs::write(backup_file, script_start_time.to_rfc3339())?;
    }

    Ok(())
}
