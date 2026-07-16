//! Durable command-log project storage with one worker-owned `SQLite` writer.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread::{self, JoinHandle};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use stitch_editor_contracts::{
    Command, ModelError, Project, RequestId, RevisionHash, reduce, revision_hash,
};

const SQLITE_VERSION: &str = "3.53.2";
const SQLITE_SOURCE_ID: &str =
    "2026-06-03 19:12:13 d6e03d8c777cfa2d35e3b60d8ec3e0187f3e9f99d8e2ee9cac695fd6fcdf1a24";
const WAL_AUTOCHECKPOINT_PAGES: i64 = 0;
const CHANNEL_CAPACITY: usize = 32;
const SNAPSHOT_INTERVAL: i64 = 128;
const STORE_SCHEMA_VERSION: &str = "1";
const APPLICATION_ID: i64 = 0x5354_4348;
const USER_VERSION: i64 = 1;
static ARCHIVE_TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
static TEST_TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);
const CREATE_REVISIONS: &str = "CREATE TABLE revisions (sequence INTEGER PRIMARY KEY NOT NULL CHECK(sequence>0), revision_hash BLOB NOT NULL CHECK(length(revision_hash)=32), parent_hash BLOB CHECK(parent_hash IS NULL OR length(parent_hash)=32), command BLOB NOT NULL CHECK(length(command)>0), UNIQUE(sequence,revision_hash), CHECK((sequence=1 AND parent_hash IS NULL) OR (sequence>1 AND parent_hash IS NOT NULL))) STRICT";
const CREATE_RECEIPTS: &str = "CREATE TABLE receipts (request_id BLOB PRIMARY KEY NOT NULL CHECK(length(request_id)=16), revision_hash BLOB NOT NULL CHECK(length(revision_hash)=32), sequence INTEGER NOT NULL CHECK(sequence>0), command BLOB NOT NULL CHECK(length(command)>0), FOREIGN KEY(sequence,revision_hash) REFERENCES revisions(sequence,revision_hash)) STRICT";
const CREATE_HEADS: &str = "CREATE TABLE heads (name TEXT PRIMARY KEY NOT NULL CHECK(name='main'), sequence INTEGER NOT NULL CHECK(sequence>0), revision_hash BLOB NOT NULL CHECK(length(revision_hash)=32), FOREIGN KEY(sequence,revision_hash) REFERENCES revisions(sequence,revision_hash)) STRICT";
const CREATE_SNAPSHOTS: &str = "CREATE TABLE snapshots (sequence INTEGER NOT NULL CHECK(sequence>0), revision_hash BLOB NOT NULL CHECK(length(revision_hash)=32), project BLOB NOT NULL CHECK(length(project)>0), project_digest BLOB NOT NULL CHECK(length(project_digest)=32), PRIMARY KEY(sequence,revision_hash), FOREIGN KEY(sequence,revision_hash) REFERENCES revisions(sequence,revision_hash)) STRICT";
const CREATE_METADATA: &str =
    "CREATE TABLE metadata (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL) STRICT";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultPoint {
    BeforeCommit,
    AfterCommitBeforeResponse,
    BeforeSnapshot,
    DuringSnapshot,
    AfterSnapshot,
    BeforeCheckpoint,
    DuringCheckpoint,
    AfterCheckpoint,
    BeforeCompaction,
    DuringCompaction,
    AfterCompaction,
    BeforeArchive,
    DuringArchive,
    AfterArchive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaintenanceError {
    Snapshot,
    Checkpoint,
    Compaction,
    Archive,
    Fault(FaultPoint),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckpointReceipt {
    pub log_frames: i64,
    pub checkpointed_frames: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitReceipt {
    pub request_id: RequestId,
    pub revision_hash: RevisionHash,
    pub sequence: i64,
}

#[derive(Debug)]
pub enum StoreError {
    Sql(rusqlite::Error),
    Model(ModelError),
    Ambiguous(RequestId),
    RequestConflict(RequestId),
    Injected(FaultPoint),
    Maintenance(MaintenanceError),
    UnsupportedSqlite(String),
    RuntimeMismatch,
    CorruptStore,
    WorkerStopped,
}
impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql(error) => write!(f, "sqlite error: {error}"),
            Self::Model(error) => write!(f, "project model error: {error}"),
            Self::Ambiguous(_) => f.write_str("commit may have completed; retry request id"),
            Self::RequestConflict(_) => {
                f.write_str("request id was already used for another command")
            }
            Self::Injected(point) => write!(f, "injected fault at {point:?}"),
            Self::Maintenance(error) => write!(f, "maintenance failed: {error:?}"),
            Self::UnsupportedSqlite(version) => write!(f, "SQLite {version} is below 3.51.3"),
            Self::RuntimeMismatch => {
                f.write_str("SQLite runtime does not match locked bundled build")
            }
            Self::CorruptStore => {
                f.write_str("project store contains invalid command log or snapshot")
            }
            Self::WorkerStopped => f.write_str("project store worker stopped"),
        }
    }
}
impl std::error::Error for StoreError {}
impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sql(error)
    }
}
impl From<ModelError> for StoreError {
    fn from(error: ModelError) -> Self {
        Self::Model(error)
    }
}

pub struct ProjectStore {
    sender: SyncSender<Work>,
    worker: Option<JoinHandle<()>>,
    database_path: PathBuf,
}

enum Work {
    Commit {
        request_id: RequestId,
        command: Command,
        fault: Option<FaultPoint>,
        response: SyncSender<Result<CommitReceipt, StoreError>>,
    },
    Receipt {
        request_id: RequestId,
        response: SyncSender<Result<Option<CommitReceipt>, StoreError>>,
    },
    Snapshot {
        fault: Option<FaultPoint>,
        response: SyncSender<Result<(), StoreError>>,
    },
    Checkpoint {
        fault: Option<FaultPoint>,
        response: SyncSender<Result<CheckpointReceipt, StoreError>>,
    },
    Compact {
        fault: Option<FaultPoint>,
        response: SyncSender<Result<(), StoreError>>,
    },
    Archive {
        destination: PathBuf,
        fault: Option<FaultPoint>,
        response: SyncSender<Result<(), StoreError>>,
    },
    Shutdown,
}

struct State {
    connection: Connection,
    project: Project,
    head: Option<RevisionHash>,
    sequence: i64,
    database_path: PathBuf,
    maintenance_failure: Option<MaintenanceError>,
}
type SnapshotRow = (i64, Vec<u8>, Vec<u8>, Vec<u8>);

impl ProjectStore {
    /// Starts the bounded single-writer worker and restores its verified in-memory head.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if the exact bundled runtime, command log, or snapshot is invalid.
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let database_path = database_path.as_ref().to_path_buf();
        let (sender, receiver) = sync_channel(CHANNEL_CAPACITY);
        let (ready_sender, ready_receiver) = sync_channel(1);
        let worker_path = database_path.clone();
        let worker = thread::Builder::new()
            .name("stitch-project-store".into())
            .spawn(move || worker_main(worker_path, receiver, ready_sender))
            .map_err(|_| StoreError::WorkerStopped)?;
        ready_receiver
            .recv()
            .map_err(|_| StoreError::WorkerStopped)??;
        Ok(Self {
            sender,
            worker: Some(worker),
            database_path,
        })
    }

    /// Queues an edit and waits for its durable receipt; `SQLite` work stays on the writer thread.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Ambiguous`] only after the transaction has committed.
    pub fn commit(
        &self,
        request_id: RequestId,
        command: &Command,
    ) -> Result<CommitReceipt, StoreError> {
        self.request_commit(request_id, command.clone(), None)
    }

    #[cfg(test)]
    fn commit_at(
        &self,
        request_id: RequestId,
        command: &Command,
        fault: FaultPoint,
    ) -> Result<CommitReceipt, StoreError> {
        self.request_commit(request_id, command.clone(), Some(fault))
    }

    fn request_commit(
        &self,
        request_id: RequestId,
        command: Command,
        fault: Option<FaultPoint>,
    ) -> Result<CommitReceipt, StoreError> {
        let (response_sender, response_receiver) = sync_channel(1);
        self.sender
            .send(Work::Commit {
                request_id,
                command,
                fault,
                response: response_sender,
            })
            .map_err(|_| StoreError::WorkerStopped)?;
        response_receiver
            .recv()
            .map_err(|_| StoreError::WorkerStopped)?
    }

    /// Reads a durable receipt through the worker-owned connection.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if the worker has stopped or the receipt is malformed.
    pub fn receipt(&self, request_id: RequestId) -> Result<Option<CommitReceipt>, StoreError> {
        let (response_sender, response_receiver) = sync_channel(1);
        self.sender
            .send(Work::Receipt {
                request_id,
                response: response_sender,
            })
            .map_err(|_| StoreError::WorkerStopped)?;
        response_receiver
            .recv()
            .map_err(|_| StoreError::WorkerStopped)?
    }

    /// Materializes the current acknowledged head as an integrity-checked snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Maintenance`] if snapshot persistence cannot be completed.
    pub fn snapshot(&self) -> Result<(), StoreError> {
        self.request_snapshot(None)
    }

    /// Checkpoints the live WAL with `TRUNCATE`; no copied database files are involved.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Maintenance`] when `SQLite` reports a busy or incomplete checkpoint.
    pub fn checkpoint(&self) -> Result<CheckpointReceipt, StoreError> {
        self.request_checkpoint(None)
    }

    /// Reclaims obsolete snapshot/free pages while preserving the complete revision authority.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Maintenance`] if the SQLite-managed compaction fails.
    pub fn compact(&self) -> Result<(), StoreError> {
        self.request_compact(None)
    }

    /// Writes a validated `SQLite` backup to `destination` through a sibling temporary artifact.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Maintenance`] if backup, validation, syncing, or publication fails.
    pub fn archive(&self, destination: impl AsRef<Path>) -> Result<(), StoreError> {
        self.request_archive(destination.as_ref().to_path_buf(), None)
    }

    #[cfg(test)]
    fn snapshot_at(&self, fault: FaultPoint) -> Result<(), StoreError> {
        self.request_snapshot(Some(fault))
    }

    #[cfg(test)]
    fn checkpoint_at(&self, fault: FaultPoint) -> Result<CheckpointReceipt, StoreError> {
        self.request_checkpoint(Some(fault))
    }

    #[cfg(test)]
    fn compact_at(&self, fault: FaultPoint) -> Result<(), StoreError> {
        self.request_compact(Some(fault))
    }

    #[cfg(test)]
    fn archive_at(&self, destination: PathBuf, fault: FaultPoint) -> Result<(), StoreError> {
        self.request_archive(destination, Some(fault))
    }

    fn request_snapshot(&self, fault: Option<FaultPoint>) -> Result<(), StoreError> {
        let (response_sender, response_receiver) = sync_channel(1);
        self.sender
            .send(Work::Snapshot {
                fault,
                response: response_sender,
            })
            .map_err(|_| StoreError::WorkerStopped)?;
        response_receiver
            .recv()
            .map_err(|_| StoreError::WorkerStopped)?
    }

    fn request_checkpoint(
        &self,
        fault: Option<FaultPoint>,
    ) -> Result<CheckpointReceipt, StoreError> {
        let (response_sender, response_receiver) = sync_channel(1);
        self.sender
            .send(Work::Checkpoint {
                fault,
                response: response_sender,
            })
            .map_err(|_| StoreError::WorkerStopped)?;
        response_receiver
            .recv()
            .map_err(|_| StoreError::WorkerStopped)?
    }

    fn request_compact(&self, fault: Option<FaultPoint>) -> Result<(), StoreError> {
        let (response_sender, response_receiver) = sync_channel(1);
        self.sender
            .send(Work::Compact {
                fault,
                response: response_sender,
            })
            .map_err(|_| StoreError::WorkerStopped)?;
        response_receiver
            .recv()
            .map_err(|_| StoreError::WorkerStopped)?
    }

    fn request_archive(
        &self,
        destination: PathBuf,
        fault: Option<FaultPoint>,
    ) -> Result<(), StoreError> {
        let (response_sender, response_receiver) = sync_channel(1);
        self.sender
            .send(Work::Archive {
                destination,
                fault,
                response: response_sender,
            })
            .map_err(|_| StoreError::WorkerStopped)?;
        response_receiver
            .recv()
            .map_err(|_| StoreError::WorkerStopped)?
    }

    /// Stops and joins the worker so database/WAL/SHM cleanup is safe on Windows.
    /// Stops and joins the writer before callers remove database files.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::WorkerStopped`] if the worker cannot be joined.
    pub fn shutdown(&mut self) -> Result<(), StoreError> {
        let _ = self.sender.send(Work::Shutdown);
        self.worker
            .take()
            .ok_or(StoreError::WorkerStopped)?
            .join()
            .map_err(|_| StoreError::WorkerStopped)
    }

    #[must_use]
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }
    #[must_use]
    pub fn live_unit_paths(&self) -> [PathBuf; 3] {
        [
            self.database_path.clone(),
            with_suffix(&self.database_path, "-wal"),
            with_suffix(&self.database_path, "-shm"),
        ]
    }
}

impl Drop for ProjectStore {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[allow(clippy::needless_pass_by_value)] // Ownership transfers into the dedicated thread.
fn worker_main(path: PathBuf, receiver: Receiver<Work>, ready: SyncSender<Result<(), StoreError>>) {
    let mut state = match open_state(&path) {
        Ok(state) => {
            let _ = ready.send(Ok(()));
            state
        }
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    while let Ok(work) = receiver.recv() {
        match work {
            Work::Commit {
                request_id,
                command,
                fault,
                response,
            } => {
                if let Some(error) = state.maintenance_failure {
                    let _ = response.send(Err(StoreError::Maintenance(error)));
                    continue;
                }
                let result = commit(&mut state, request_id, &command, fault);
                let snapshot_due = result.is_ok() && state.sequence % SNAPSHOT_INTERVAL == 0;
                let _ = response.send(result);
                let snapshot_failure = snapshot_due
                    .then(|| persist_snapshot(&mut state, None))
                    .and_then(Result::err);
                if let Some(StoreError::Maintenance(error)) = snapshot_failure {
                    state.maintenance_failure = Some(error);
                }
            }
            Work::Receipt {
                request_id,
                response,
            } => {
                let _ = response.send(receipt(&state.connection, request_id));
            }
            Work::Snapshot { fault, response } => {
                let result = match state.maintenance_failure {
                    Some(error) => Err(StoreError::Maintenance(error)),
                    None => persist_snapshot(&mut state, fault),
                };
                if let Err(StoreError::Maintenance(error)) = &result {
                    state.maintenance_failure = Some(*error);
                }
                let _ = response.send(result);
            }
            Work::Checkpoint { fault, response } => {
                let result = match state.maintenance_failure {
                    Some(error) => Err(StoreError::Maintenance(error)),
                    None => checkpoint(&state.connection, fault),
                };
                let _ = response.send(result);
            }
            Work::Compact { fault, response } => {
                let result = match state.maintenance_failure {
                    Some(error) => Err(StoreError::Maintenance(error)),
                    None => compact(&mut state, fault),
                };
                let _ = response.send(result);
            }
            Work::Archive {
                destination,
                fault,
                response,
            } => {
                let result = match state.maintenance_failure {
                    Some(error) => Err(StoreError::Maintenance(error)),
                    None => archive(&state, &destination, fault),
                };
                let _ = response.send(result);
            }
            Work::Shutdown => break,
        }
    }
}

fn open_state(path: &Path) -> Result<State, StoreError> {
    let mut connection = Connection::open(path)?;
    verify_runtime(&connection)?;
    let application_id: i64 =
        connection.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    let user_version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let objects: i64 = connection.query_row(
        "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    let initialize = application_id == 0 && user_version == 0 && objects == 0;
    if initialize {
        initialize_store(&mut connection)?;
    } else if application_id != APPLICATION_ID || user_version != USER_VERSION {
        return Err(StoreError::CorruptStore);
    }
    configure_connection(&connection)?;
    validate_schema(&connection)?;
    let mode: String = connection.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    let sync: i64 = connection.query_row("PRAGMA synchronous", [], |row| row.get(0))?;
    let checkpoint: i64 =
        connection.query_row("PRAGMA wal_autocheckpoint", [], |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") || sync != 2 || checkpoint != WAL_AUTOCHECKPOINT_PAGES {
        return Err(StoreError::CorruptStore);
    }
    let quick_check: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    let foreign_key_error: Option<String> = connection
        .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
        .optional()?;
    if quick_check != "ok" || foreign_key_error.is_some() {
        return Err(StoreError::CorruptStore);
    }
    let (project, head, sequence) = restore(&connection)?;
    Ok(State {
        connection,
        project,
        head,
        sequence,
        database_path: path.to_path_buf(),
        maintenance_failure: None,
    })
}

fn initialize_store(connection: &mut Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA wal_autocheckpoint=0;",
    )?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(&format!(
        "{CREATE_REVISIONS}; {CREATE_RECEIPTS}; {CREATE_HEADS}; {CREATE_SNAPSHOTS}; {CREATE_METADATA}; PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version={USER_VERSION};"
    ))?;
    transaction.execute(
        "INSERT INTO metadata(key,value) VALUES('schema_version',?1)",
        [STORE_SCHEMA_VERSION],
    )?;
    transaction.commit()?;
    Ok(())
}

fn configure_connection(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA wal_autocheckpoint=0;",
    )?;
    Ok(())
}

fn validate_schema(connection: &Connection) -> Result<(), StoreError> {
    let actual = {
        let mut statement = connection.prepare(
            "SELECT type,name,sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY name",
        )?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    let expected = [
        ("heads", CREATE_HEADS),
        ("metadata", CREATE_METADATA),
        ("receipts", CREATE_RECEIPTS),
        ("revisions", CREATE_REVISIONS),
        ("snapshots", CREATE_SNAPSHOTS),
    ];
    if actual.len() != expected.len()
        || actual.iter().zip(expected).any(|(actual, expected)| {
            actual.0 != "table"
                || actual.1 != expected.0
                || actual.2.as_deref().map(normalize_sql) != Some(normalize_sql(expected.1))
        })
    {
        return Err(StoreError::CorruptStore);
    }
    let metadata = {
        let mut statement = connection.prepare("SELECT key,value FROM metadata ORDER BY key")?;
        statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    if metadata.as_slice() != [("schema_version".to_owned(), STORE_SCHEMA_VERSION.to_owned())] {
        return Err(StoreError::CorruptStore);
    }
    Ok(())
}

fn normalize_sql(sql: &str) -> String {
    sql.chars()
        .filter(|character| !character.is_ascii_whitespace())
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

fn verify_runtime(connection: &Connection) -> Result<(), StoreError> {
    let version: String = connection.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
    let source: String = connection.query_row("SELECT sqlite_source_id()", [], |row| row.get(0))?;
    let options = {
        let mut statement = connection.prepare("PRAGMA compile_options")?;
        statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    if version != SQLITE_VERSION
        || source != SQLITE_SOURCE_ID
        || !options.iter().any(|option| option == "THREADSAFE=1")
        || options
            .iter()
            .any(|option| option == "OMIT_WAL" || option == "OMIT_COMPILEOPTION_DIAGS")
    {
        return Err(StoreError::RuntimeMismatch);
    }
    Ok(())
}

fn restore(connection: &Connection) -> Result<(Project, Option<RevisionHash>, i64), StoreError> {
    let expected_head: Option<(i64, Vec<u8>)> = connection
        .query_row(
            "SELECT sequence,revision_hash FROM heads WHERE name='main'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let snapshot: Option<SnapshotRow> = connection.query_row("SELECT sequence,revision_hash,project,project_digest FROM snapshots ORDER BY sequence DESC LIMIT 1", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
    if let Some((sequence, hash, bytes, digest)) = &snapshot {
        let snapshot_project =
            Project::from_canonical_bytes(bytes).map_err(|_| StoreError::CorruptStore)?;
        if snapshot_project.semantic_digest().0.as_slice() != digest.as_slice()
            || hash_from_bytes(hash).is_err()
            || connection
                .query_row(
                    "SELECT 1 FROM revisions WHERE sequence=?1 AND revision_hash=?2",
                    params![sequence, hash],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .is_none()
        {
            return Err(StoreError::CorruptStore);
        }
    }
    let mut statement = connection.prepare(
        "SELECT sequence, revision_hash, parent_hash, command FROM revisions ORDER BY sequence",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Vec<u8>>(1)?,
            row.get::<_, Option<Vec<u8>>>(2)?,
            row.get::<_, Vec<u8>>(3)?,
        ))
    })?;
    let mut project = Project::default();
    let mut head = None;
    let mut sequence = 0;
    let mut matched_snapshot = snapshot.is_none();
    for row in rows {
        let (next_sequence, hash, parent, command) = row?;
        if next_sequence != sequence + 1 {
            return Err(StoreError::CorruptStore);
        }
        let command =
            Command::from_canonical_bytes(&command).map_err(|_| StoreError::CorruptStore)?;
        let parent = parent.map(|bytes| hash_from_bytes(&bytes)).transpose()?;
        if parent != head {
            return Err(StoreError::CorruptStore);
        }
        let expected = revision_hash(head, &command);
        if expected != hash_from_bytes(&hash)? {
            return Err(StoreError::CorruptStore);
        }
        project = reduce(&project, &command).map_err(|_| StoreError::CorruptStore)?;
        head = Some(expected);
        sequence = next_sequence;
        match &snapshot {
            Some((snapshot_sequence, snapshot_hash, snapshot_project, snapshot_digest))
                if sequence == *snapshot_sequence =>
            {
                if head != Some(hash_from_bytes(snapshot_hash)?)
                    || project.canonical_bytes().as_slice() != snapshot_project.as_slice()
                    || project.semantic_digest().0.as_slice() != snapshot_digest.as_slice()
                {
                    return Err(StoreError::CorruptStore);
                }
                matched_snapshot = true;
            }
            _ => {}
        }
    }
    if !matched_snapshot {
        return Err(StoreError::CorruptStore);
    }
    validate_receipts(connection)?;
    match expected_head {
        None if sequence == 0 => Ok((project, head, sequence)),
        Some((head_sequence, hash))
            if head_sequence == sequence && Some(hash_from_bytes(&hash)?) == head =>
        {
            Ok((project, head, sequence))
        }
        _ => Err(StoreError::CorruptStore),
    }
}

fn commit(
    state: &mut State,
    request_id: RequestId,
    command: &Command,
    fault: Option<FaultPoint>,
) -> Result<CommitReceipt, StoreError> {
    if let Some(receipt) = receipt(&state.connection, request_id)? {
        let stored: Vec<u8> = state.connection.query_row(
            "SELECT command FROM receipts WHERE request_id=?1",
            [request_id.0.to_vec()],
            |row| row.get(0),
        )?;
        return if stored == command.canonical_bytes() {
            Ok(receipt)
        } else {
            Err(StoreError::RequestConflict(request_id))
        };
    }
    let next = reduce(&state.project, command)?;
    let hash = revision_hash(state.head, command);
    let sequence = state
        .sequence
        .checked_add(1)
        .ok_or(StoreError::CorruptStore)?;
    let receipt = CommitReceipt {
        request_id,
        revision_hash: hash,
        sequence,
    };
    let transaction = state
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute(
        "INSERT INTO revisions(sequence, revision_hash, parent_hash, command) VALUES(?1,?2,?3,?4)",
        params![
            sequence,
            hash.0.to_vec(),
            state.head.map(|value| value.0.to_vec()),
            command.canonical_bytes()
        ],
    )?;
    transaction.execute(
        "INSERT INTO receipts(request_id, revision_hash, sequence, command) VALUES(?1,?2,?3,?4)",
        params![
            request_id.0.to_vec(),
            hash.0.to_vec(),
            sequence,
            command.canonical_bytes()
        ],
    )?;
    transaction.execute("INSERT INTO heads(name,sequence,revision_hash) VALUES('main',?1,?2) ON CONFLICT(name) DO UPDATE SET sequence=excluded.sequence,revision_hash=excluded.revision_hash", params![sequence, hash.0.to_vec()])?;
    if fault == Some(FaultPoint::BeforeCommit) {
        return Err(StoreError::Injected(FaultPoint::BeforeCommit));
    }
    transaction.commit()?;
    state.project = next;
    state.head = Some(hash);
    state.sequence = sequence;
    if fault == Some(FaultPoint::AfterCommitBeforeResponse) {
        return Err(StoreError::Ambiguous(request_id));
    }
    Ok(receipt)
}

fn persist_snapshot(state: &mut State, fault: Option<FaultPoint>) -> Result<(), StoreError> {
    if state.sequence == 0 {
        return Ok(());
    }
    maintenance_fault(fault, FaultPoint::BeforeSnapshot)?;
    let head = state.head.ok_or(StoreError::CorruptStore)?;
    let exists: Option<i64> = state
        .connection
        .query_row(
            "SELECT 1 FROM snapshots WHERE sequence=?1 AND revision_hash=?2",
            params![state.sequence, head.0.to_vec()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| StoreError::Maintenance(MaintenanceError::Snapshot))?;
    if exists.is_some() {
        return Ok(());
    }
    maintenance_fault(fault, FaultPoint::DuringSnapshot)?;
    let transaction = state
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| StoreError::Maintenance(MaintenanceError::Snapshot))?;
    transaction.execute(
        "INSERT OR IGNORE INTO snapshots(sequence,revision_hash,project,project_digest) VALUES(?1,?2,?3,?4)",
        params![
            state.sequence,
            head.0.to_vec(),
            state.project.canonical_bytes(),
            state.project.semantic_digest().0.to_vec()
        ],
    ).map_err(|_| StoreError::Maintenance(MaintenanceError::Snapshot))?;
    transaction
        .commit()
        .map_err(|_| StoreError::Maintenance(MaintenanceError::Snapshot))?;
    maintenance_fault(fault, FaultPoint::AfterSnapshot)?;
    Ok(())
}

fn checkpoint(
    connection: &Connection,
    fault: Option<FaultPoint>,
) -> Result<CheckpointReceipt, StoreError> {
    maintenance_fault(fault, FaultPoint::BeforeCheckpoint)?;
    maintenance_fault(fault, FaultPoint::DuringCheckpoint)?;
    let (busy, log_frames, checkpointed_frames): (i64, i64, i64) = connection
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|_| StoreError::Maintenance(MaintenanceError::Checkpoint))?;
    if busy != 0 || log_frames != checkpointed_frames {
        return Err(StoreError::Maintenance(MaintenanceError::Checkpoint));
    }
    maintenance_fault(fault, FaultPoint::AfterCheckpoint)?;
    Ok(CheckpointReceipt {
        log_frames,
        checkpointed_frames,
    })
}

fn compact(state: &mut State, fault: Option<FaultPoint>) -> Result<(), StoreError> {
    maintenance_fault(fault, FaultPoint::BeforeCompaction)?;
    persist_snapshot(state, None)?;
    let transaction = state
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| StoreError::Maintenance(MaintenanceError::Compaction))?;
    transaction
        .execute(
            "DELETE FROM snapshots WHERE sequence < (SELECT max(sequence) FROM snapshots)",
            [],
        )
        .map_err(|_| StoreError::Maintenance(MaintenanceError::Compaction))?;
    transaction
        .commit()
        .map_err(|_| StoreError::Maintenance(MaintenanceError::Compaction))?;
    maintenance_fault(fault, FaultPoint::DuringCompaction)?;
    checkpoint(&state.connection, None)?;
    state
        .connection
        .execute_batch("VACUUM")
        .map_err(|_| StoreError::Maintenance(MaintenanceError::Compaction))?;
    maintenance_fault(fault, FaultPoint::AfterCompaction)?;
    Ok(())
}

fn archive(state: &State, destination: &Path, fault: Option<FaultPoint>) -> Result<(), StoreError> {
    if destination == state.database_path
        || destination == with_suffix(&state.database_path, "-wal")
        || destination == with_suffix(&state.database_path, "-shm")
        || destination.exists()
    {
        return Err(StoreError::Maintenance(MaintenanceError::Archive));
    }
    maintenance_fault(fault, FaultPoint::BeforeArchive)?;
    let temporary = archive_temporary_path(destination)?;
    state
        .connection
        .backup(rusqlite::MAIN_DB, &temporary, None)
        .map_err(|_| StoreError::Maintenance(MaintenanceError::Archive))?;
    maintenance_fault(fault, FaultPoint::DuringArchive)?;
    let validation = validate_archive(&temporary, state);
    if validation.is_err() {
        return Err(StoreError::Maintenance(MaintenanceError::Archive));
    }
    OpenOptions::new()
        .write(true)
        .open(&temporary)
        .and_then(|file| file.sync_all())
        .map_err(|_| StoreError::Maintenance(MaintenanceError::Archive))?;
    maintenance_fault(fault, FaultPoint::AfterArchive)?;
    fs::rename(&temporary, destination)
        .map_err(|_| StoreError::Maintenance(MaintenanceError::Archive))?;
    sync_archive_parent(destination)?;
    Ok(())
}

fn sync_archive_parent(destination: &Path) -> Result<(), StoreError> {
    #[cfg(unix)]
    {
        let parent = destination
            .parent()
            .ok_or(StoreError::Maintenance(MaintenanceError::Archive))?;
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| StoreError::Maintenance(MaintenanceError::Archive))?;
    }
    #[cfg(not(unix))]
    {
        // Windows does not expose a portable directory fsync through std, but
        // the archive contract still rejects a destination without a parent.
        let _ = destination
            .parent()
            .ok_or(StoreError::Maintenance(MaintenanceError::Archive))?;
    }
    Ok(())
}

fn validate_archive(path: &Path, state: &State) -> Result<(), StoreError> {
    let connection = Connection::open(path)?;
    verify_runtime(&connection)?;
    validate_schema(&connection)?;
    let quick_check: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if quick_check != "ok" {
        return Err(StoreError::CorruptStore);
    }
    let (project, head, sequence) = restore(&connection)?;
    if sequence != state.sequence
        || head != state.head
        || project.semantic_digest() != state.project.semantic_digest()
    {
        return Err(StoreError::CorruptStore);
    }
    Ok(())
}

fn archive_temporary_path(destination: &Path) -> Result<PathBuf, StoreError> {
    let parent = destination
        .parent()
        .ok_or(StoreError::Maintenance(MaintenanceError::Archive))?;
    let name = destination
        .file_name()
        .ok_or(StoreError::Maintenance(MaintenanceError::Archive))?;
    let mut temporary = name.to_os_string();
    temporary.push(format!(
        ".tmp-{}-{}",
        std::process::id(),
        ARCHIVE_TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary = parent.join(temporary);
    if temporary.exists() {
        return Err(StoreError::Maintenance(MaintenanceError::Archive));
    }
    Ok(temporary)
}

fn maintenance_fault(fault: Option<FaultPoint>, point: FaultPoint) -> Result<(), StoreError> {
    if fault == Some(point) {
        return Err(StoreError::Maintenance(MaintenanceError::Fault(point)));
    }
    Ok(())
}

fn validate_receipts(connection: &Connection) -> Result<(), StoreError> {
    let receipt_count: i64 =
        connection.query_row("SELECT count(*) FROM receipts", [], |row| row.get(0))?;
    let revision_count: i64 =
        connection.query_row("SELECT count(*) FROM revisions", [], |row| row.get(0))?;
    let mismatch: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM receipts AS receipt LEFT JOIN revisions AS revision ON revision.sequence=receipt.sequence AND revision.revision_hash=receipt.revision_hash WHERE revision.sequence IS NULL OR receipt.command != revision.command LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if receipt_count != revision_count || mismatch.is_some() {
        return Err(StoreError::CorruptStore);
    }
    Ok(())
}

fn receipt(
    connection: &Connection,
    request_id: RequestId,
) -> Result<Option<CommitReceipt>, StoreError> {
    let row: Option<(Vec<u8>, i64)> = connection
        .query_row(
            "SELECT revision_hash,sequence FROM receipts WHERE request_id=?1",
            [request_id.0.to_vec()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    row.map(|(hash, sequence)| {
        Ok(CommitReceipt {
            request_id,
            revision_hash: hash_from_bytes(&hash)?,
            sequence,
        })
    })
    .transpose()
}
fn hash_from_bytes(bytes: &[u8]) -> Result<RevisionHash, StoreError> {
    Ok(RevisionHash(
        bytes.try_into().map_err(|_| StoreError::CorruptStore)?,
    ))
}
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value: OsString = path.as_os_str().to_owned();
    value.push(suffix);
    PathBuf::from(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command as ProcessCommand;
    use std::time::{SystemTime, UNIX_EPOCH};
    use stitch_editor_contracts::{Asset, AssetId, AssetIdentity};
    #[test]
    fn retry_and_reopen_replays_command_log() {
        let directory = temp();
        let path = directory.join("project.sqlite");
        let command = asset(2);
        let request = RequestId([1; 16]);
        let mut store = ProjectStore::open(&path).unwrap();
        assert!(matches!(
            store.commit_at(request, &command, FaultPoint::AfterCommitBeforeResponse),
            Err(StoreError::Ambiguous(_))
        ));
        store.shutdown().unwrap();
        let mut reopened = ProjectStore::open(&path).unwrap();
        let receipt = reopened.commit(request, &command).unwrap();
        assert_eq!(receipt.sequence, 1);
        reopened.shutdown().unwrap();
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn precommit_is_not_durable_and_request_conflicts() {
        let directory = temp();
        let path = directory.join("project.sqlite");
        let mut store = ProjectStore::open(&path).unwrap();
        assert!(matches!(
            store.commit_at(RequestId([2; 16]), &asset(3), FaultPoint::BeforeCommit),
            Err(StoreError::Injected(_))
        ));
        assert!(store.receipt(RequestId([2; 16])).unwrap().is_none());
        store.commit(RequestId([3; 16]), &asset(4)).unwrap();
        assert!(matches!(
            store.commit(RequestId([3; 16]), &asset(5)),
            Err(StoreError::RequestConflict(_))
        ));
        store.shutdown().unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reopens_across_multiple_snapshot_intervals() {
        let directory = temp();
        let path = directory.join("project.sqlite");
        let mut store = ProjectStore::open(&path).unwrap();
        for value in 1..=257_u16 {
            let id: [u8; 16] = value.to_be_bytes().repeat(8).try_into().unwrap();
            store
                .commit(
                    RequestId(id),
                    &Command::AddTrack {
                        track_id: stitch_editor_contracts::TrackId(id),
                    },
                )
                .unwrap();
        }
        store.shutdown().unwrap();
        let mut reopened = ProjectStore::open(&path).unwrap();
        assert_eq!(
            reopened
                .receipt(RequestId(
                    257_u16.to_be_bytes().repeat(8).try_into().unwrap()
                ))
                .unwrap()
                .unwrap()
                .sequence,
            257
        );
        reopened.shutdown().unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reopens_reject_snapshot_digest_hash_sequence_and_project_tampering() {
        for statement in [
            "UPDATE snapshots SET project_digest=zeroblob(32)",
            "UPDATE snapshots SET revision_hash=zeroblob(32)",
            "UPDATE snapshots SET sequence=127",
            "UPDATE snapshots SET project=x'01'",
        ] {
            let directory = temp();
            let path = directory.join("project.sqlite");
            create_snapshot_store(&path);
            tamper(&path, statement);
            assert_rejected(&path);
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn reopens_reject_head_sequence_and_hash_tampering() {
        for statement in [
            "UPDATE heads SET sequence=1",
            "UPDATE heads SET revision_hash=zeroblob(32)",
        ] {
            let directory = temp();
            let path = directory.join("project.sqlite");
            create_log_store(&path, 2);
            tamper(&path, statement);
            assert_rejected(&path);
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn reopens_reject_revision_command_hash_parent_gap_and_trailing_row_tampering() {
        for statement in [
            "UPDATE revisions SET command=x'01' WHERE sequence=2",
            "UPDATE revisions SET revision_hash=zeroblob(32) WHERE sequence=2",
            "UPDATE revisions SET parent_hash=zeroblob(32) WHERE sequence=2",
            "UPDATE revisions SET sequence=3 WHERE sequence=2",
        ] {
            let directory = temp();
            let path = directory.join("project.sqlite");
            create_log_store(&path, 2);
            tamper(&path, statement);
            assert_rejected(&path);
            fs::remove_dir_all(directory).unwrap();
        }

        let directory = temp();
        let path = directory.join("project.sqlite");
        create_log_store(&path, 1);
        let connection = Connection::open(&path).unwrap();
        let parent: Vec<u8> = connection
            .query_row(
                "SELECT revision_hash FROM revisions WHERE sequence=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let command = asset(50);
        let hash = revision_hash(Some(hash_from_bytes(&parent).unwrap()), &command);
        connection
            .execute_batch("PRAGMA foreign_keys=OFF;")
            .unwrap();
        connection
            .execute(
                "INSERT INTO revisions(sequence,revision_hash,parent_hash,command) VALUES(2,?1,?2,?3)",
                params![hash.0.to_vec(), parent, command.canonical_bytes()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO receipts(request_id,revision_hash,sequence,command) VALUES(?1,?2,2,?3)",
                params![RequestId([50; 16]).0.to_vec(), hash.0.to_vec(), command.canonical_bytes()],
            )
            .unwrap();
        drop(connection);
        assert_rejected(&path);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reopens_reject_receipt_command_sequence_and_hash_tampering() {
        for statement in [
            "UPDATE receipts SET command=x'01' WHERE sequence=2",
            "UPDATE receipts SET sequence=1 WHERE sequence=2",
            "UPDATE receipts SET revision_hash=zeroblob(32) WHERE sequence=2",
        ] {
            let directory = temp();
            let path = directory.join("project.sqlite");
            create_log_store(&path, 2);
            tamper(&path, statement);
            assert_rejected(&path);
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn reopens_reject_schema_and_application_id_tampering() {
        for statement in [
            "UPDATE metadata SET value='0' WHERE key='schema_version'",
            "CREATE TABLE unexpected (value TEXT NOT NULL)",
            "PRAGMA application_id=0",
            "PRAGMA user_version=0",
        ] {
            let directory = temp();
            let path = directory.join("project.sqlite");
            create_log_store(&path, 1);
            tamper(&path, statement);
            assert_rejected(&path);
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn unrelated_database_is_rejected_without_changes() {
        let directory = temp();
        let path = directory.join("unrelated.sqlite");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("CREATE TABLE unrelated (value TEXT NOT NULL); INSERT INTO unrelated VALUES('keep');")
            .unwrap();
        drop(connection);
        let before = fs::read(&path).unwrap();
        assert_rejected(&path);
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(!with_suffix(&path, "-wal").exists());
        assert!(!with_suffix(&path, "-shm").exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reconfigure_each_open_restores_wal_mode() {
        let directory = temp();
        let path = directory.join("project.sqlite");
        create_log_store(&path, 1);
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("PRAGMA journal_mode=DELETE;")
            .unwrap();
        drop(connection);
        let mut store = ProjectStore::open(&path).unwrap();
        store.shutdown().unwrap();
        let connection = Connection::open(&path).unwrap();
        let mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert!(mode.eq_ignore_ascii_case("wal"));
        drop(connection);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn snapshot_checkpoint_compaction_and_archive_preserve_replay_authority() {
        let directory = temp();
        let path = directory.join("project.sqlite");
        let archive_path = directory.join("archive.sqlite");
        let mut store = ProjectStore::open(&path).unwrap();
        store.commit(RequestId([61; 16]), &asset(61)).unwrap();
        store.commit(RequestId([62; 16]), &asset(62)).unwrap();
        store.snapshot().unwrap();
        store.snapshot().unwrap();
        let checkpoint = store.checkpoint().unwrap();
        assert_eq!(checkpoint.log_frames, checkpoint.checkpointed_frames);
        store.archive(&archive_path).unwrap();
        store.compact().unwrap();
        assert_eq!(
            store
                .receipt(RequestId([62; 16]))
                .unwrap()
                .unwrap()
                .sequence,
            2
        );
        store.shutdown().unwrap();
        let mut archive = ProjectStore::open(&archive_path).unwrap();
        assert_eq!(
            archive
                .receipt(RequestId([62; 16]))
                .unwrap()
                .unwrap()
                .sequence,
            2
        );
        archive.shutdown().unwrap();
        let mut reopened = ProjectStore::open(&path).unwrap();
        assert_eq!(
            reopened
                .receipt(RequestId([62; 16]))
                .unwrap()
                .unwrap()
                .sequence,
            2
        );
        reopened.shutdown().unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn maintenance_faults_leave_acknowledged_commands_recoverable() {
        for fault in [
            FaultPoint::BeforeCheckpoint,
            FaultPoint::DuringCheckpoint,
            FaultPoint::AfterCheckpoint,
            FaultPoint::BeforeCompaction,
            FaultPoint::DuringCompaction,
            FaultPoint::AfterCompaction,
        ] {
            let directory = temp();
            let path = directory.join("project.sqlite");
            let mut store = ProjectStore::open(&path).unwrap();
            store.commit(RequestId([71; 16]), &asset(71)).unwrap();
            let result = match fault {
                FaultPoint::BeforeCheckpoint
                | FaultPoint::DuringCheckpoint
                | FaultPoint::AfterCheckpoint => store.checkpoint_at(fault).map(|_| ()),
                FaultPoint::BeforeCompaction
                | FaultPoint::DuringCompaction
                | FaultPoint::AfterCompaction => store.compact_at(fault),
                _ => unreachable!(),
            };
            assert!(matches!(
                result,
                Err(StoreError::Maintenance(MaintenanceError::Fault(point))) if point == fault
            ));
            store.shutdown().unwrap();
            let mut reopened = ProjectStore::open(&path).unwrap();
            assert_eq!(
                reopened
                    .receipt(RequestId([71; 16]))
                    .unwrap()
                    .unwrap()
                    .sequence,
                1
            );
            reopened.shutdown().unwrap();
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn snapshot_failure_blocks_mutation_but_not_receipt_recovery() {
        for fault in [
            FaultPoint::BeforeSnapshot,
            FaultPoint::DuringSnapshot,
            FaultPoint::AfterSnapshot,
        ] {
            let directory = temp();
            let path = directory.join("project.sqlite");
            let mut store = ProjectStore::open(&path).unwrap();
            store.commit(RequestId([81; 16]), &asset(81)).unwrap();
            assert!(matches!(
                store.snapshot_at(fault),
                Err(StoreError::Maintenance(MaintenanceError::Fault(point))) if point == fault
            ));
            assert!(matches!(
                store.commit(RequestId([82; 16]), &asset(82)),
                Err(StoreError::Maintenance(MaintenanceError::Fault(point))) if point == fault
            ));
            assert_eq!(
                store
                    .receipt(RequestId([81; 16]))
                    .unwrap()
                    .unwrap()
                    .sequence,
                1
            );
            store.shutdown().unwrap();
            let mut reopened = ProjectStore::open(&path).unwrap();
            assert_eq!(
                reopened
                    .receipt(RequestId([81; 16]))
                    .unwrap()
                    .unwrap()
                    .sequence,
                1
            );
            reopened.shutdown().unwrap();
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn archive_faults_do_not_publish_partial_artifacts() {
        for fault in [
            FaultPoint::BeforeArchive,
            FaultPoint::DuringArchive,
            FaultPoint::AfterArchive,
        ] {
            let directory = temp();
            let path = directory.join("project.sqlite");
            let archive_path = directory.join("archive.sqlite");
            let mut store = ProjectStore::open(&path).unwrap();
            store.commit(RequestId([91; 16]), &asset(91)).unwrap();
            assert!(matches!(
                store.archive_at(archive_path.clone(), fault),
                Err(StoreError::Maintenance(MaintenanceError::Fault(point))) if point == fault
            ));
            assert!(!archive_path.exists());
            store.shutdown().unwrap();
            let mut reopened = ProjectStore::open(&path).unwrap();
            assert_eq!(
                reopened
                    .receipt(RequestId([91; 16]))
                    .unwrap()
                    .unwrap()
                    .sequence,
                1
            );
            reopened.shutdown().unwrap();
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn child_process_abort_recovers_commit_and_maintenance_boundaries() {
        if let (Ok(path), Ok(case)) = (
            std::env::var("STITCH_STORE_CHILD_CRASH_PATH"),
            std::env::var("STITCH_STORE_CHILD_CRASH_CASE"),
        ) {
            child_termination_case(Path::new(&path), &case);
            std::process::abort();
        }
        for case in [
            "before-commit",
            "after-commit-before-response",
            "snapshot-after",
            "checkpoint-after",
            "compaction-after",
            "archive-after",
        ] {
            let directory = temp();
            let path = directory.join("project.sqlite");
            let status = ProcessCommand::new(std::env::current_exe().unwrap())
                .arg("--exact")
                .arg("tests::child_process_abort_recovers_commit_and_maintenance_boundaries")
                .arg("--nocapture")
                .env("STITCH_STORE_CHILD_CRASH_PATH", &path)
                .env("STITCH_STORE_CHILD_CRASH_CASE", case)
                .status()
                .unwrap();
            assert!(
                !status.success(),
                "child case {case} unexpectedly exited cleanly"
            );
            let mut reopened = ProjectStore::open(&path).unwrap();
            match case {
                "before-commit" => {
                    assert!(reopened.receipt(RequestId([101; 16])).unwrap().is_none());
                    assert_eq!(
                        reopened
                            .commit(RequestId([101; 16]), &asset(101))
                            .unwrap()
                            .sequence,
                        1
                    );
                }
                "after-commit-before-response" => {
                    assert_eq!(
                        reopened
                            .receipt(RequestId([102; 16]))
                            .unwrap()
                            .unwrap()
                            .sequence,
                        1
                    );
                    assert_eq!(
                        reopened
                            .commit(RequestId([102; 16]), &asset(102))
                            .unwrap()
                            .sequence,
                        1
                    );
                }
                "archive-after" => {
                    assert!(!path.with_extension("archive.sqlite").exists());
                    assert_eq!(
                        reopened
                            .receipt(RequestId([103; 16]))
                            .unwrap()
                            .unwrap()
                            .sequence,
                        1
                    );
                }
                "snapshot-after" | "checkpoint-after" | "compaction-after" => {
                    assert_eq!(
                        reopened
                            .receipt(RequestId([103; 16]))
                            .unwrap()
                            .unwrap()
                            .sequence,
                        1
                    );
                }
                _ => unreachable!(),
            }
            reopened.shutdown().unwrap();
            fs::remove_dir_all(directory).unwrap();
        }
    }

    fn child_termination_case(path: &Path, case: &str) {
        let store = ProjectStore::open(path).unwrap();
        match case {
            "before-commit" => assert!(matches!(
                store.commit_at(RequestId([101; 16]), &asset(101), FaultPoint::BeforeCommit),
                Err(StoreError::Injected(FaultPoint::BeforeCommit))
            )),
            "after-commit-before-response" => assert!(matches!(
                store.commit_at(
                    RequestId([102; 16]),
                    &asset(102),
                    FaultPoint::AfterCommitBeforeResponse
                ),
                Err(StoreError::Ambiguous(_))
            )),
            "snapshot-after" => {
                store.commit(RequestId([103; 16]), &asset(103)).unwrap();
                assert!(matches!(
                    store.snapshot_at(FaultPoint::AfterSnapshot),
                    Err(StoreError::Maintenance(MaintenanceError::Fault(
                        FaultPoint::AfterSnapshot
                    )))
                ));
            }
            "checkpoint-after" => {
                store.commit(RequestId([103; 16]), &asset(103)).unwrap();
                assert!(matches!(
                    store.checkpoint_at(FaultPoint::AfterCheckpoint),
                    Err(StoreError::Maintenance(MaintenanceError::Fault(
                        FaultPoint::AfterCheckpoint
                    )))
                ));
            }
            "compaction-after" => {
                store.commit(RequestId([103; 16]), &asset(103)).unwrap();
                assert!(matches!(
                    store.compact_at(FaultPoint::AfterCompaction),
                    Err(StoreError::Maintenance(MaintenanceError::Fault(
                        FaultPoint::AfterCompaction
                    )))
                ));
            }
            "archive-after" => {
                store.commit(RequestId([103; 16]), &asset(103)).unwrap();
                assert!(matches!(
                    store.archive_at(
                        path.with_extension("archive.sqlite"),
                        FaultPoint::AfterArchive
                    ),
                    Err(StoreError::Maintenance(MaintenanceError::Fault(
                        FaultPoint::AfterArchive
                    )))
                ));
            }
            _ => panic!("unexpected child case"),
        }
    }

    fn create_log_store(path: &Path, revisions: u8) {
        let mut store = ProjectStore::open(path).unwrap();
        for value in 1..=revisions {
            store.commit(RequestId([value; 16]), &asset(value)).unwrap();
        }
        store.shutdown().unwrap();
    }

    fn create_snapshot_store(path: &Path) {
        let mut store = ProjectStore::open(path).unwrap();
        for value in 1..=128_u16 {
            let id: [u8; 16] = value.to_be_bytes().repeat(8).try_into().unwrap();
            store
                .commit(
                    RequestId(id),
                    &Command::AddTrack {
                        track_id: stitch_editor_contracts::TrackId(id),
                    },
                )
                .unwrap();
        }
        store.shutdown().unwrap();
    }

    fn tamper(path: &Path, statement: &str) {
        let connection = Connection::open(path).unwrap();
        connection
            .execute_batch(&format!("PRAGMA foreign_keys=OFF; {statement};"))
            .unwrap();
    }

    fn assert_rejected(path: &Path) {
        assert!(matches!(
            ProjectStore::open(path),
            Err(StoreError::CorruptStore)
        ));
    }
    fn asset(value: u8) -> Command {
        Command::AddAsset {
            asset: Asset::new(
                AssetId([value; 16]),
                AssetIdentity {
                    content_sha256: [value; 32],
                    byte_length: u64::from(value),
                    provider_id: [9; 16],
                },
            ),
        }
    }
    fn temp() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "stitch-store-{}-{nanos}-{}",
            std::process::id(),
            TEST_TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }
}
