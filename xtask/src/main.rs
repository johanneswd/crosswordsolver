use std::collections::{HashMap, HashSet, VecDeque};
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use cargo_metadata::{Dependency, Metadata, MetadataCommand, Package, PackageId};
use clap::{Parser, Subcommand};
use semver::Version;

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Workspace maintenance utilities")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    CheckTag {
        #[arg(long)]
        tag: String,
    },
    Publish {
        #[arg(long)]
        tag: String,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
}

#[derive(Clone)]
struct PublishablePackage {
    id: PackageId,
    name: String,
    version: Version,
    manifest_path: String,
    dependencies: Vec<Dependency>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::CheckTag { tag } => {
            let info = check_tag(&tag)?;
            println!(
                "Tag {} matches workspace version {} across {} publishable crates.",
                tag,
                info.version,
                info.packages.len()
            );
        }
        Commands::Publish { tag, dry_run } => publish(&tag, dry_run)?,
    }

    Ok(())
}

struct WorkspaceInfo {
    version: Version,
    packages: Vec<PublishablePackage>,
}

fn check_tag(tag: &str) -> Result<WorkspaceInfo> {
    let tag_version = parse_tag(tag)?;
    let metadata = load_metadata()?;
    let packages = collect_publishable(&metadata)?;

    if packages.is_empty() {
        bail!("No publishable workspace packages found.");
    }

    let expected_version = packages[0].version.clone();
    let mut mismatches = Vec::new();

    for pkg in &packages {
        if pkg.version != expected_version {
            mismatches.push(pkg);
        }
    }

    if !mismatches.is_empty() {
        eprintln!("Version mismatch across publishable crates:");
        for pkg in mismatches {
            eprintln!(
                "- {} ({}): found {}, expected {}",
                pkg.name, pkg.manifest_path, pkg.version, expected_version
            );
        }
        bail!("Publishable crate versions are not aligned.");
    }

    if expected_version != tag_version {
        bail!(
            "Tag {} does not match workspace version {}.",
            tag,
            expected_version
        );
    }

    Ok(WorkspaceInfo {
        version: expected_version,
        packages,
    })
}

fn publish(tag: &str, dry_run: bool) -> Result<()> {
    let info = check_tag(tag)?;
    let ordered = topological_sort(&info.packages)?;

    for pkg in ordered {
        println!("Publishing {} {}", pkg.name, pkg.version);
        run_publish_command(&pkg, true)?;
        if dry_run {
            continue;
        }

        let backoff = [3, 8, 20];
        for (idx, delay) in backoff.iter().enumerate() {
            match run_publish_command(&pkg, false) {
                Ok(()) => break,
                Err(err) if idx < backoff.len() - 1 && err_is_retryable(&err) => {
                    println!(
                        "Retryable publish error for {}: {}. Retrying in {}s...",
                        pkg.name, err, delay
                    );
                    thread::sleep(Duration::from_secs(*delay));
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
    }

    println!("Publish sequence complete.");
    Ok(())
}

fn parse_tag(tag: &str) -> Result<Version> {
    if let Some(stripped) = tag.strip_prefix('v') {
        Version::parse(stripped).context("Failed to parse semver from tag")
    } else {
        bail!("Tag must start with 'v' (e.g., v1.2.3).");
    }
}

fn load_metadata() -> Result<Metadata> {
    MetadataCommand::new()
        .no_deps()
        .exec()
        .context("Failed to load cargo metadata")
}

fn collect_publishable(metadata: &Metadata) -> Result<Vec<PublishablePackage>> {
    let workspace_ids: HashSet<_> = metadata.workspace_members.iter().collect();
    let mut packages = Vec::new();

    for pkg in &metadata.packages {
        if !workspace_ids.contains(&pkg.id) {
            continue;
        }

        if pkg.name == "xtask" {
            continue;
        }

        if !is_publishable(pkg) {
            continue;
        }

        packages.push(PublishablePackage {
            id: pkg.id.clone(),
            name: pkg.name.clone(),
            version: pkg.version.clone(),
            manifest_path: pkg
                .manifest_path
                .as_std_path()
                .to_string_lossy()
                .to_string(),
            dependencies: pkg.dependencies.clone(),
        });
    }

    Ok(packages)
}

fn is_publishable(pkg: &Package) -> bool {
    match &pkg.publish {
        None => true,
        Some(registries) => !registries.is_empty(),
    }
}

fn topological_sort(packages: &[PublishablePackage]) -> Result<Vec<PublishablePackage>> {
    let mut package_map: HashMap<&PackageId, &PublishablePackage> =
        packages.iter().map(|p| (&p.id, p)).collect();
    let publishable_ids: HashSet<&PackageId> = package_map.keys().cloned().collect();
    let name_to_id: HashMap<&str, &PackageId> =
        packages.iter().map(|p| (p.name.as_str(), &p.id)).collect();

    let mut adj: HashMap<&PackageId, Vec<&PackageId>> = HashMap::new();
    let mut indegree: HashMap<&PackageId, usize> = HashMap::new();
    for id in &publishable_ids {
        indegree.insert(*id, 0);
    }

    for pkg in packages {
        let mut seen = HashSet::new();
        for dep in &pkg.dependencies {
            if let Some(&dep_id) = name_to_id.get(dep.name.as_str())
                && publishable_ids.contains(dep_id) && seen.insert(dep_id) {
                    adj.entry(dep_id).or_default().push(&pkg.id);
                    *indegree.entry(&pkg.id).or_default() += 1;
                }
        }
    }

    let mut queue: VecDeque<&PackageId> = indegree
        .iter()
        .filter_map(|(id, &deg)| if deg == 0 { Some(*id) } else { None })
        .collect();
    let mut ordered = Vec::with_capacity(packages.len());

    while let Some(id) = queue.pop_front() {
        let pkg = package_map
            .remove(id)
            .context("Package missing from map during sort")?;
        ordered.push(pkg.clone());

        if let Some(dependents) = adj.get(id) {
            for dep_id in dependents {
                if let Some(entry) = indegree.get_mut(dep_id) {
                    *entry -= 1;
                    if *entry == 0 {
                        queue.push_back(dep_id);
                    }
                }
            }
        }
    }

    if ordered.len() != packages.len() {
        bail!("Cycle detected in workspace dependencies among publishable crates.");
    }

    Ok(ordered)
}

fn run_publish_command(pkg: &PublishablePackage, dry_run: bool) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.args(["publish", "-p", &pkg.name, "--locked"]);
    if dry_run {
        cmd.arg("--dry-run");
    }

    let output = cmd
        .output()
        .with_context(|| format!("Failed to run cargo publish for {}", pkg.name))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    if output.status.success() {
        return Ok(());
    }

    if combined.contains("already uploaded") || combined.contains("is already uploaded") {
        println!("{} {} already uploaded; continuing.", pkg.name, pkg.version);
        return Ok(());
    }

    if should_retry(&combined) {
        bail!("{}", combined.trim());
    }

    bail!(
        "cargo publish failed for {}: {}\n{}",
        pkg.name,
        stderr.trim(),
        stdout.trim()
    );
}

fn should_retry(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("no matching package named")
        || lower.contains("failed to select a version for the requirement")
}

fn err_is_retryable(err: &anyhow::Error) -> bool {
    should_retry(&err.to_string())
}
