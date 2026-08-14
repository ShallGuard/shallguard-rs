//! Atomic local checkpoints and portable content-addressed review caching.

use std::fs::{File, OpenOptions};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::validation::{CapsuleMetadata, validate_response};
use super::{
    BundleEntry, BundleManifest, REVIEW_PROTOCOL, ReviewEntry, ReviewOptions, ReviewOrigin,
    ReviewProvider, ReviewResult, ReviewStatus, digest, unix_timestamp,
};

const RUN_STATE_SCHEMA: &str = "shallguard.requirement-review-run-state/v1";
const CHECKPOINT_SCHEMA: &str = "shallguard.requirement-review-checkpoint/v1";
const CACHE_SCHEMA: &str = "shallguard.requirement-review-cache/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RunUnit {
    requirement: String,
    capsule_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RunIdentity {
    protocol: String,
    bundle_manifest_digest: String,
    provider: ReviewProvider,
    model: String,
    local_provider: Option<String>,
    cli_version: String,
    units: Vec<RunUnit>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RunState {
    schema: String,
    identity: RunIdentity,
    started_unix_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct UnitCheckpoint {
    schema: String,
    cache_key: String,
    review: ReviewEntry,
}

#[derive(Debug, Serialize)]
struct CacheKey<'a> {
    schema: &'static str,
    capsule_digest: &'a str,
    protocol: &'static str,
    prompt_digest: &'a str,
    response_schema_digest: &'a str,
    provider: ReviewProvider,
    model: &'a str,
    local_provider: Option<&'a str>,
    cli_version: &'a str,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheRecord {
    schema: String,
    cache_key: String,
    response_digest: String,
    duration_ms: u64,
}

pub(super) enum Reuse<T> {
    Hit(T),
    Miss,
    Invalid(String),
}

pub(super) struct CachedUnit {
    pub(super) result: ReviewResult,
    pub(super) response_digest: String,
    pub(super) duration_ms: u64,
}

pub(super) struct Attempt {
    pub(super) number: u32,
    pub(super) dir: PathBuf,
    pub(super) result_file: String,
}

pub(super) struct ReviewStore<'a> {
    output_dir: &'a Path,
    cache_dir: Option<&'a Path>,
    identity: RunIdentity,
    resume: bool,
    _lock: File,
}

impl<'a> ReviewStore<'a> {
    #[shallguard_macros::enforces("REQ-REV-007")]
    pub(super) fn open(
        options: &'a ReviewOptions<'_>,
        manifest: &BundleManifest,
        entries: &[BundleEntry],
        cli_version: &str,
    ) -> Result<Self> {
        let identity = RunIdentity {
            protocol: REVIEW_PROTOCOL.to_string(),
            bundle_manifest_digest: manifest.digest.clone(),
            provider: options.provider,
            model: options.model.unwrap_or("configured-default").to_string(),
            local_provider: options.local_provider.map(str::to_string),
            cli_version: cli_version.to_string(),
            units: entries
                .iter()
                .map(|entry| RunUnit {
                    requirement: entry.requirement.clone(),
                    capsule_digest: entry.digest.clone(),
                })
                .collect(),
        };
        let existed = options
            .output_dir
            .try_exists()
            .with_context(|| format!("checking review output {}", options.output_dir.display()))?;
        if existed && !options.resume {
            bail!(
                "review output {} already exists; pass --resume for a compatible run or choose another path",
                options.output_dir.display()
            );
        }
        if !existed {
            create_parent(options.output_dir)?;
            std::fs::create_dir(options.output_dir).with_context(|| {
                format!("creating review output {}", options.output_dir.display())
            })?;
        }
        let run_path = options.output_dir.join("run.json");
        if existed
            && !run_path
                .try_exists()
                .with_context(|| format!("checking review run state {}", run_path.display()))?
        {
            bail!(
                "review output {} has no run.json and cannot be resumed; it may predate the \
                 resumable protocol, so retain it and choose a new --output path",
                options.output_dir.display()
            );
        }
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(options.output_dir.join(".lock"))
            .context("opening review run lock")?;
        lock.try_lock().with_context(|| {
            format!(
                "review output {} is already in use by another process",
                options.output_dir.display()
            )
        })?;
        std::fs::create_dir_all(options.output_dir.join("units"))
            .context("creating review unit directory")?;
        if let Some(cache_dir) = options.cache_dir {
            std::fs::create_dir_all(cache_dir).with_context(|| {
                format!("creating review cache directory {}", cache_dir.display())
            })?;
        }
        if existed {
            let state: RunState = read_json(&run_path)?;
            if state.schema != RUN_STATE_SCHEMA {
                bail!("unsupported review run state schema {:?}", state.schema);
            }
            if state.identity != identity {
                bail!("existing review output does not match this bundle/provider/model selection");
            }
        } else {
            write_json_atomic(
                &run_path,
                &RunState {
                    schema: RUN_STATE_SCHEMA.to_string(),
                    identity: identity.clone(),
                    started_unix_seconds: unix_timestamp()?,
                },
            )?;
        }
        Ok(Self {
            output_dir: options.output_dir,
            cache_dir: options.cache_dir,
            identity,
            resume: options.resume,
            _lock: lock,
        })
    }

    pub(super) fn cache_key(
        &self,
        entry: &BundleEntry,
        prompt_digest: &str,
        response_schema_digest: &str,
    ) -> Result<String> {
        let bytes = serde_json::to_vec(&CacheKey {
            schema: CACHE_SCHEMA,
            capsule_digest: &entry.digest,
            protocol: REVIEW_PROTOCOL,
            prompt_digest,
            response_schema_digest,
            provider: self.identity.provider,
            model: &self.identity.model,
            local_provider: self.identity.local_provider.as_deref(),
            cli_version: &self.identity.cli_version,
        })
        .context("serializing review cache key")?;
        Ok(digest(&bytes))
    }

    pub(super) fn checkpoint(
        &self,
        entry: &BundleEntry,
        cache_key: &str,
        metadata: &CapsuleMetadata,
    ) -> Reuse<ReviewEntry> {
        if !self.resume {
            return Reuse::Miss;
        }
        match self.read_checkpoint(entry, cache_key, metadata) {
            Ok(Some(review)) => Reuse::Hit(review),
            Ok(None) => Reuse::Miss,
            Err(error) => Reuse::Invalid(format!("{error:#}")),
        }
    }

    fn read_checkpoint(
        &self,
        entry: &BundleEntry,
        cache_key: &str,
        metadata: &CapsuleMetadata,
    ) -> Result<Option<ReviewEntry>> {
        let path = self.unit_dir(entry).join("checkpoint.json");
        if !path
            .try_exists()
            .with_context(|| format!("checking {}", path.display()))?
        {
            return Ok(None);
        }
        let checkpoint: UnitCheckpoint = read_json(&path)?;
        if checkpoint.schema != CHECKPOINT_SCHEMA || checkpoint.cache_key != cache_key {
            bail!("checkpoint identity does not match the current review unit");
        }
        if checkpoint.review.status != ReviewStatus::Completed {
            bail!("checkpoint is not a completed review");
        }
        if checkpoint.review.requirement_id != entry.requirement
            || checkpoint.review.capsule_file != entry.file
            || checkpoint.review.capsule_digest != entry.digest
        {
            bail!("checkpoint review identity does not match the current capsule");
        }
        let result_path = self.completed_result_path(&checkpoint.review)?;
        let result: ReviewResult = read_json(&result_path)?;
        let result = validate_response(result, metadata)
            .map_err(anyhow::Error::from)
            .context("revalidating checkpoint response")?;
        validate_result_summary(&checkpoint.review, &result)?;
        let mut review = checkpoint.review;
        review.origin = ReviewOrigin::Resumed;
        Ok(Some(review))
    }

    pub(super) fn read_result(&self, review: &ReviewEntry) -> Result<Option<ReviewResult>> {
        if review.status == ReviewStatus::Failed {
            return Ok(None);
        }
        let result: ReviewResult = read_json(&self.completed_result_path(review)?)?;
        validate_result_summary(review, &result)?;
        Ok(Some(result))
    }

    #[shallguard_macros::enforces("REQ-REV-008")]
    pub(super) fn cache(&self, cache_key: &str, metadata: &CapsuleMetadata) -> Reuse<CachedUnit> {
        let Some(path) = self.cache_entry_dir(cache_key) else {
            return Reuse::Miss;
        };
        match read_cached_unit(&path, cache_key, metadata) {
            Ok(Some(unit)) => Reuse::Hit(unit),
            Ok(None) => Reuse::Miss,
            Err(error) => Reuse::Invalid(format!("{error:#}")),
        }
    }

    pub(super) fn start_attempt(&self, entry: &BundleEntry) -> Result<Attempt> {
        let attempts = self.unit_dir(entry).join("attempts");
        std::fs::create_dir_all(&attempts).with_context(|| {
            format!("creating review attempts directory {}", attempts.display())
        })?;
        let next = std::fs::read_dir(&attempts)
            .with_context(|| format!("reading {}", attempts.display()))?
            .filter_map(|item| item.ok())
            .filter_map(|item| item.file_name().to_str()?.parse::<u32>().ok())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let name = format!("{next:04}");
        let dir = attempts.join(&name);
        std::fs::create_dir(&dir)
            .with_context(|| format!("creating review attempt {}", dir.display()))?;
        Ok(Attempt {
            number: next,
            dir,
            result_file: format!("units/{}/attempts/{name}/result.json", entry.requirement),
        })
    }

    pub(super) fn write_attempt(&self, attempt: &Attempt, review: &ReviewEntry) -> Result<()> {
        write_json_atomic(&attempt.dir.join("attempt.json"), review)
    }

    #[shallguard_macros::enforces("REQ-REV-006")]
    pub(super) fn write_checkpoint(
        &self,
        entry: &BundleEntry,
        cache_key: &str,
        review: &ReviewEntry,
    ) -> Result<()> {
        write_json_atomic(
            &self.unit_dir(entry).join("checkpoint.json"),
            &UnitCheckpoint {
                schema: CHECKPOINT_SCHEMA.to_string(),
                cache_key: cache_key.to_string(),
                review: review.clone(),
            },
        )
    }

    pub(super) fn write_cache(
        &self,
        cache_key: &str,
        result: &ReviewResult,
        response_digest: &str,
        duration_ms: u64,
    ) -> Result<()> {
        let Some(path) = self.cache_entry_dir(cache_key) else {
            return Ok(());
        };
        if path.join("metadata.json").try_exists().unwrap_or(false) {
            return Ok(());
        }
        std::fs::create_dir_all(&path)
            .with_context(|| format!("creating cache entry {}", path.display()))?;
        write_json_atomic(&path.join("result.json"), result)?;
        write_json_atomic(
            &path.join("metadata.json"),
            &CacheRecord {
                schema: CACHE_SCHEMA.to_string(),
                cache_key: cache_key.to_string(),
                response_digest: response_digest.to_string(),
                duration_ms,
            },
        )
    }

    fn unit_dir(&self, entry: &BundleEntry) -> PathBuf {
        self.output_dir.join("units").join(&entry.requirement)
    }

    fn completed_result_path(&self, review: &ReviewEntry) -> Result<PathBuf> {
        let result_file = review
            .result_file
            .as_deref()
            .context("completed review has no result file")?;
        let expected_result_file = format!(
            "units/{}/attempts/{:04}/result.json",
            review.requirement_id, review.attempt
        );
        if result_file != expected_result_file {
            bail!("review result path does not match its requirement and attempt");
        }
        safe_output_path(self.output_dir, result_file)
    }

    fn cache_entry_dir(&self, cache_key: &str) -> Option<PathBuf> {
        let cache_dir = self.cache_dir?;
        let key = cache_key.strip_prefix("sha256:").unwrap_or(cache_key);
        let shard = key.get(..2).unwrap_or("00");
        Some(cache_dir.join("v1").join(shard).join(key))
    }
}

fn validate_result_summary(review: &ReviewEntry, result: &ReviewResult) -> Result<()> {
    let response_digest =
        digest(&serde_json::to_vec(result).context("serializing stored response digest")?);
    if review.response_digest.as_deref() != Some(response_digest.as_str()) {
        bail!("review response digest does not match its result file");
    }
    if review.requirement_id != result.requirement_id
        || review.capsule_digest != result.capsule_digest
        || review.verdict != Some(result.verdict)
        || review.confidence != Some(result.confidence)
        || review.failure_kind.is_some()
        || review.error.is_some()
    {
        bail!("review summary does not match its validated result");
    }
    Ok(())
}

fn create_parent(path: &Path) -> Result<()> {
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(());
    };
    std::fs::create_dir_all(parent)
        .with_context(|| format!("creating output parent {}", parent.display()))
}

#[shallguard_macros::enforces("REQ-REV-008", "REQ-SEC-005")]
fn read_cached_unit(
    path: &Path,
    cache_key: &str,
    metadata: &CapsuleMetadata,
) -> Result<Option<CachedUnit>> {
    let metadata_path = path.join("metadata.json");
    if !metadata_path
        .try_exists()
        .with_context(|| format!("checking {}", metadata_path.display()))?
    {
        return Ok(None);
    }
    let record: CacheRecord = read_json(&metadata_path)?;
    if record.schema != CACHE_SCHEMA || record.cache_key != cache_key {
        bail!("cache entry identity does not match its path");
    }
    let result: ReviewResult = read_json(&path.join("result.json"))?;
    let result = validate_response(result, metadata)
        .map_err(anyhow::Error::from)
        .context("revalidating cached response")?;
    let response_digest =
        digest(&serde_json::to_vec(&result).context("serializing cached response digest")?);
    if response_digest != record.response_digest {
        bail!("cached response digest does not match result.json");
    }
    Ok(Some(CachedUnit {
        result,
        response_digest,
        duration_ms: record.duration_ms,
    }))
}

#[shallguard_macros::enforces("REQ-SEC-002")]
fn safe_output_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("unsafe result path in checkpoint: {relative:?}");
    }
    Ok(root.join(path))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

pub(super) fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut json = serde_json::to_string_pretty(value)
        .with_context(|| format!("serializing {}", path.display()))?;
    json.push('\n');
    write_atomic(path, json.as_bytes())
}

pub(super) fn write_atomic(path: &Path, content: &[u8]) -> Result<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("atomic output path has no UTF-8 file name")?;
    let temp = path.with_file_name(format!(".{file_name}.tmp-{}", std::process::id()));
    std::fs::write(&temp, content).with_context(|| format!("writing {}", temp.display()))?;
    std::fs::rename(&temp, path)
        .with_context(|| format!("publishing atomic output {}", path.display()))
}

#[cfg(test)]
#[path = "review_state_tests.rs"]
mod tests;
