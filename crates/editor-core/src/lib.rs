//! Pure, deterministic project history. UI and media execution stay outside it.

#![forbid(unsafe_code)]

use stitch_editor_contracts::{Command, ModelError, Project, RevisionHash, reduce, revision_hash};

/// Version of the workspace/build contract, not a project-model schema.
pub const BOOTSTRAP_CONTRACT_VERSION: u32 = 1;

/// Immutable-revision history with deterministic undo and redo cursor movement.
#[derive(Clone, Debug, Default)]
pub struct EditorHistory {
    revisions: Vec<HistoryRevision>,
    cursor: usize,
}

#[derive(Clone, Debug)]
struct HistoryRevision {
    project: Project,
    hash: RevisionHash,
}

impl EditorHistory {
    #[must_use]
    pub fn project(&self) -> Project {
        self.cursor
            .checked_sub(1)
            .and_then(|index| self.revisions.get(index))
            .map_or_else(Project::default, |revision| revision.project.clone())
    }

    #[must_use]
    pub fn current_hash(&self) -> Option<RevisionHash> {
        self.cursor
            .checked_sub(1)
            .and_then(|index| self.revisions.get(index))
            .map(|revision| revision.hash)
    }

    #[must_use]
    pub fn revision_count(&self) -> usize {
        self.revisions.len()
    }

    /// Applies a typed command, replacing any redo branch with a new revision.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError`] without changing history if command preconditions
    /// do not hold.
    pub fn apply(&mut self, command: &Command) -> Result<RevisionHash, ModelError> {
        let project = self.project();
        let next = reduce(&project, command)?;
        let revision = HistoryRevision {
            hash: revision_hash(self.current_hash(), command),
            project: next,
        };
        self.revisions.truncate(self.cursor);
        let hash = revision.hash;
        self.revisions.push(revision);
        self.cursor += 1;
        Ok(hash)
    }

    #[must_use]
    pub fn undo(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        true
    }

    #[must_use]
    pub fn redo(&mut self) -> bool {
        if self.cursor == self.revisions.len() {
            return false;
        }
        self.cursor += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use stitch_editor_contracts::{
        Asset, AssetId, AssetIdentity, ClipId, Command, ModelError, Rational, RevisionHash,
        TimeRange, TrackId,
    };

    use super::EditorHistory;

    #[test]
    fn mixed_thousand_command_replay_undo_redo_and_branch_are_deterministic() {
        let commands = mixed_commands(200);
        assert_eq!(commands.len(), 1_000);
        let mut first = EditorHistory::default();
        for command in &commands {
            first.apply(command).unwrap();
        }
        let final_hash = first.current_hash();
        let final_project = first.project();
        assert_eq!(first.revision_count(), 1_000);
        assert_eq!(
            final_hash,
            Some(RevisionHash([
                0x5e, 0xaa, 0x34, 0xd5, 0xcf, 0xb3, 0x7f, 0x9d, 0x32, 0x70, 0x72, 0x3a, 0x8f, 0x29,
                0x9a, 0xc5, 0x01, 0xfe, 0x8e, 0x42, 0xc4, 0x16, 0x19, 0xac, 0xbc, 0x73, 0x00, 0xe5,
                0x95, 0xa2, 0xc8, 0x1e,
            ]))
        );

        for _ in 0..1_000 {
            assert!(first.undo());
        }
        assert_eq!(first.current_hash(), None);
        assert!(!first.undo());
        for _ in 0..1_000 {
            assert!(first.redo());
        }
        assert_eq!(first.current_hash(), final_hash);
        assert_eq!(first.project(), final_project);
        assert!(!first.redo());

        let mut replay = EditorHistory::default();
        for command in &commands {
            replay.apply(command).unwrap();
        }
        assert_eq!(replay.current_hash(), final_hash);
        assert_eq!(replay.project(), final_project);

        for _ in 0..10 {
            assert!(first.undo());
        }
        let old_redo_hash = final_hash;
        first
            .apply(&Command::AddAsset {
                asset: indexed_asset(AssetId(id_bytes(9, 0)), 250),
            })
            .unwrap();
        assert_eq!(first.revision_count(), 991);
        assert!(!first.redo());
        assert_ne!(first.current_hash(), old_redo_hash);
    }

    #[test]
    fn invalid_precondition_does_not_change_history_or_project() {
        let mut history = EditorHistory::default();
        history
            .apply(&Command::AddAsset {
                asset: asset(AssetId([1; 16])),
            })
            .unwrap();
        let before_project = history.project();
        let before_hash = history.current_hash();
        assert_eq!(
            history.apply(&Command::AddAsset {
                asset: asset(AssetId([1; 16])),
            }),
            Err(ModelError::DuplicateAsset(AssetId([1; 16])))
        );
        assert_eq!(history.project(), before_project);
        assert_eq!(history.current_hash(), before_hash);
        assert_eq!(history.revision_count(), 1);

        let missing_track = Command::AddClip {
            clip_id: ClipId([2; 16]),
            asset_id: AssetId([1; 16]),
            track_id: TrackId([3; 16]),
            timeline_range: unit_range(),
            source_range: unit_range(),
        };
        assert_eq!(
            history.apply(&missing_track),
            Err(ModelError::MissingTrack(TrackId([3; 16])))
        );
        assert_eq!(history.project(), before_project);
        assert_eq!(history.current_hash(), before_hash);
        assert_eq!(history.revision_count(), 1);
    }

    #[test]
    fn ten_thousand_retained_revisions_use_persistent_project_state() {
        let mut history = EditorHistory::default();
        for index in 0..10_000_u64 {
            history
                .apply(&Command::AddAsset {
                    asset: indexed_asset(AssetId(id_bytes(8, index)), index),
                })
                .unwrap();
        }
        assert_eq!(history.revision_count(), 10_000);
        assert_eq!(history.project().asset_count(), 10_000);
        assert!(history.undo());
        assert_eq!(history.project().asset_count(), 9_999);
        assert!(history.redo());
        assert_eq!(history.project().asset_count(), 10_000);
    }

    #[test]
    fn one_thousand_seeded_short_sequences_replay_undo_and_redo() {
        for seed in 0..1_000_u64 {
            let commands = cycle_commands(seed);
            let mut history = EditorHistory::default();
            for command in &commands {
                history.apply(command).unwrap();
            }
            let expected_hash = history.current_hash();
            let expected_project = history.project();
            for _ in &commands {
                assert!(history.undo());
            }
            assert_eq!(history.current_hash(), None);
            for _ in &commands {
                assert!(history.redo());
            }
            assert_eq!(history.current_hash(), expected_hash);
            assert_eq!(history.project(), expected_project);

            let mut replay = EditorHistory::default();
            for command in &commands {
                replay.apply(command).unwrap();
            }
            assert_eq!(replay.current_hash(), expected_hash);
            assert_eq!(replay.project(), expected_project);
        }
    }

    fn mixed_commands(cycles: u64) -> Vec<Command> {
        let mut commands = Vec::with_capacity(usize::try_from(cycles * 5).unwrap());
        for index in 0..cycles {
            commands.extend(cycle_commands(index));
        }
        commands
    }

    fn cycle_commands(index: u64) -> [Command; 5] {
        let asset_id = AssetId(id_bytes(1, index));
        let track_id = TrackId(id_bytes(2, index));
        let clip_id = ClipId(id_bytes(3, index));
        [
            Command::AddAsset {
                asset: indexed_asset(asset_id, index),
            },
            Command::AddTrack { track_id },
            Command::AddClip {
                clip_id,
                asset_id,
                track_id,
                timeline_range: unit_range(),
                source_range: unit_range(),
            },
            Command::MoveClip {
                clip_id,
                timeline_range: TimeRange::new(
                    Rational::new(1, 2).unwrap(),
                    Rational::new(3, 2).unwrap(),
                )
                .unwrap(),
            },
            Command::RemoveClip { clip_id },
        ]
    }

    fn id_bytes(namespace: u8, value: u64) -> [u8; 16] {
        let mut result = [0; 16];
        result[..8].fill(namespace);
        result[8..].copy_from_slice(&value.to_be_bytes());
        result
    }

    fn unit_range() -> TimeRange {
        TimeRange::new(Rational::new(0, 1).unwrap(), Rational::new(1, 1).unwrap()).unwrap()
    }

    fn indexed_asset(id: AssetId, index: u64) -> Asset {
        Asset::new(
            id,
            AssetIdentity {
                content_sha256: [u8::try_from(index % 256).unwrap(); 32],
                byte_length: index + 1,
                provider_id: [4; 16],
            },
        )
    }

    fn asset(id: AssetId) -> Asset {
        Asset::new(
            id,
            AssetIdentity {
                content_sha256: [id.0[0]; 32],
                byte_length: u64::from(id.0[0]),
                provider_id: [0; 16],
            },
        )
    }
}
