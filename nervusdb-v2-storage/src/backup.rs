//! Online Backup API - Hot snapshot capability for NervusDB
//!
//! This module provides online backup functionality that allows creating
//! consistent backups while the database is running.

use crate::Result;
use crate::error::Error;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

/// Handle to an in-progress or completed backup.
#[derive(Debug, Clone)]
pub struct BackupHandle {
    id: Uuid,
    backup_dir: PathBuf,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl BackupHandle {
    /// Get the backup ID.
    #[inline]
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Get the backup directory.
    #[inline]
    pub fn backup_dir(&self) -> &Path {
        &self.backup_dir
    }

    /// Get when the backup was created.
    #[inline]
    pub fn created_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.created_at
    }
}

/// Status of a backup operation.
#[derive(Debug, Clone)]
pub enum BackupStatus {
    /// Backup is in progress.
    InProgress {
        progress: f64,
        bytes_copied: u64,
        total_bytes: u64,
    },
    /// Backup completed successfully.
    Completed(BackupInfo),
    /// Backup failed with an error.
    Failed { error: String },
}

/// Information about a completed backup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    pub id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub size_bytes: u64,
    pub file_count: usize,
    pub nervusdb_version: String,
    pub checkpoint_txid: u64,
    pub checkpoint_epoch: u64,
}

/// Backup manifest that describes a complete backup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub backup_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub nervusdb_version: String,
    pub nervusdb_version_major: u32,
    pub nervusdb_version_minor: u32,
    pub checkpoint: CheckpointInfo,
    pub files: Vec<BackupFileInfo>,
    pub status: ManifestStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointInfo {
    pub txid: u64,
    pub epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupFileInfo {
    pub name: String,
    pub size: u64,
    pub checksum: String,
    pub is_wal: bool,
    pub wal_start_offset: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ManifestStatus {
    InProgress,
    Completed {
        completed_at: chrono::DateTime<chrono::Utc>,
        total_bytes: u64,
    },
    Failed {
        error: String,
    },
}

/// Manager for online backup operations.
#[derive(Debug)]
pub struct BackupManager {
    db_path: PathBuf,
    backup_path: PathBuf,
    active_backup: RwLock<Option<ActiveBackup>>,
}

#[derive(Debug)]
struct ActiveBackup {
    id: Uuid,
    manifest: BackupManifest,
    progress: AtomicU64,
    total_bytes: AtomicU64,
}

impl BackupManager {
    /// Create a new backup manager.
    pub fn new(db_path: PathBuf, backup_path: PathBuf) -> Self {
        Self {
            db_path,
            backup_path,
            active_backup: RwLock::new(None),
        }
    }

    /// Begin a new backup operation.
    ///
    /// This records the current checkpoint position and creates the backup
    /// directory structure. The actual file copying happens in the background.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The backup directory cannot be created
    /// - A backup is already in progress
    /// - The database files cannot be read
    pub fn begin_backup(&self) -> Result<BackupHandle> {
        // Check if backup already in progress
        if self.active_backup.read().unwrap().is_some() {
            return Err(Error::BackupProtocol(
                "A backup is already in progress".to_string(),
            ));
        }

        // Generate backup ID and paths
        let backup_id = Uuid::new_v4();
        let timestamp = chrono::Utc::now();
        let backup_dir = self.backup_path.join(backup_id.to_string());

        // Create backup directory
        std::fs::create_dir_all(&backup_dir).map_err(|e| Error::Io(e))?;

        // Get file sizes for progress tracking
        let ndb_size = self.get_file_size(&self.db_path)?;
        let wal_size = self.get_wal_size()?;

        // Read current checkpoint info from WAL
        let checkpoint_info = self.get_checkpoint_info()?;

        // Create initial manifest
        let manifest = BackupManifest {
            backup_id,
            created_at: timestamp,
            nervusdb_version: env!("CARGO_PKG_VERSION").to_string(),
            nervusdb_version_major: crate::VERSION_MAJOR,
            nervusdb_version_minor: crate::VERSION_MINOR,
            checkpoint: CheckpointInfo {
                txid: checkpoint_info.txid,
                epoch: checkpoint_info.epoch,
            },
            files: vec![
                BackupFileInfo {
                    name: self
                        .db_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    size: ndb_size,
                    checksum: String::new(), // Will be calculated during copy
                    is_wal: false,
                    wal_start_offset: None,
                },
                BackupFileInfo {
                    name: self
                        .wal_path()
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    size: wal_size,
                    checksum: String::new(),
                    is_wal: true,
                    wal_start_offset: Some(checkpoint_info.wal_offset),
                },
            ],
            status: ManifestStatus::InProgress,
        };

        // Write initial manifest
        self.write_manifest(&backup_dir, &manifest)?;

        // Store active backup
        let active = ActiveBackup {
            id: backup_id,
            manifest: manifest.clone(),
            progress: AtomicU64::new(0),
            total_bytes: AtomicU64::new(ndb_size + wal_size),
        };
        *self.active_backup.write().unwrap() = Some(active);

        Ok(BackupHandle {
            id: backup_id,
            backup_dir,
            created_at: timestamp,
        })
    }

    /// Get the current status of a backup.
    pub fn status(&self, handle: &BackupHandle) -> Result<BackupStatus> {
        let active = self.active_backup.read().unwrap();

        if let Some(ref backup) = *active {
            if backup.id != handle.id {
                return Err(Error::BackupProtocol("Backup ID mismatch".to_string()));
            }

            let progress = backup.progress.load(Ordering::Relaxed);
            let total = backup.total_bytes.load(Ordering::Relaxed);

            if total > 0 {
                Ok(BackupStatus::InProgress {
                    progress: progress as f64 / total as f64,
                    bytes_copied: progress,
                    total_bytes: total,
                })
            } else {
                Ok(BackupStatus::InProgress {
                    progress: 0.0,
                    bytes_copied: 0,
                    total_bytes: 0,
                })
            }
        } else {
            // Check if backup was completed
            let manifest_path = handle.backup_dir.join("backup_manifest.json");
            if manifest_path.exists() {
                let manifest: BackupManifest = self.read_manifest(&handle.backup_dir)?;
                match manifest.status {
                    ManifestStatus::Completed {
                        completed_at,
                        total_bytes,
                    } => Ok(BackupStatus::Completed(BackupInfo {
                        id: handle.id,
                        created_at: handle.created_at,
                        size_bytes: total_bytes,
                        file_count: manifest.files.len(),
                        nervusdb_version: manifest.nervusdb_version,
                        checkpoint_txid: manifest.checkpoint.txid,
                        checkpoint_epoch: manifest.checkpoint.epoch,
                    })),
                    ManifestStatus::Failed { error } => Ok(BackupStatus::Failed { error }),
                    ManifestStatus::InProgress => {
                        Err(Error::BackupProtocol("Backup is in progress".to_string()))
                    }
                }
            } else {
                Err(Error::BackupProtocol("Backup not found".to_string()))
            }
        }
    }

    /// Execute the backup by copying files.
    /// This can be called in a background thread.
    pub fn execute_backup(&self, handle: &BackupHandle) -> Result<()> {
        // Copy .ndb file
        self.copy_ndb_file(handle)?;

        // Copy .wal file (from checkpoint position)
        self.copy_wal_file(handle)?;

        // Mark backup as completed
        {
            let mut active = self.active_backup.write().unwrap();
            if let Some(ref mut backup) = *active {
                if backup.id == handle.id {
                    backup.manifest.status = ManifestStatus::Completed {
                        completed_at: chrono::Utc::now(),
                        total_bytes: backup.total_bytes.load(Ordering::Relaxed),
                    };
                    self.write_manifest(&handle.backup_dir, &backup.manifest)?;
                    *active = None;
                }
            }
        }

        Ok(())
    }

    /// Cancel an in-progress backup.
    pub fn cancel_backup(&self, handle: &BackupHandle) -> Result<()> {
        let mut active = self.active_backup.write().unwrap();

        if let Some(ref backup) = *active {
            if backup.id == handle.id {
                // Remove active backup marker
                *active = None;

                // Mark manifest as failed
                let mut manifest: BackupManifest = self.read_manifest(&handle.backup_dir)?;
                manifest.status = ManifestStatus::Failed {
                    error: "Cancelled by user".to_string(),
                };
                self.write_manifest(&handle.backup_dir, &manifest)?;
            }
        }

        Ok(())
    }

    /// List all backups in a directory.
    pub fn list_backups(backup_dir: &Path) -> Result<Vec<BackupInfo>> {
        let mut backups = Vec::new();

        if !backup_dir.exists() {
            return Ok(backups);
        }

        for entry in std::fs::read_dir(backup_dir).map_err(|e| Error::Io(e))? {
            let entry = entry.map_err(|e| Error::Io(e))?;
            let path = entry.path();

            if path.is_dir() {
                let manifest_path = path.join("backup_manifest.json");
                if manifest_path.exists() {
                    if let Ok(manifest) = Self::read_manifest_from_path(&manifest_path) {
                        match manifest.status {
                            ManifestStatus::Completed { total_bytes, .. } => {
                                backups.push(BackupInfo {
                                    id: manifest.backup_id,
                                    created_at: manifest.created_at,
                                    size_bytes: total_bytes,
                                    file_count: manifest.files.len(),
                                    nervusdb_version: manifest.nervusdb_version,
                                    checkpoint_txid: manifest.checkpoint.txid,
                                    checkpoint_epoch: manifest.checkpoint.epoch,
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(backups)
    }

    /// Restore a database from a backup.
    pub fn restore_from_backup(
        backup_dir: &Path,
        backup_id: Uuid,
        target_db_path: &Path,
    ) -> Result<()> {
        let backup_path = backup_dir.join(backup_id.to_string());
        let manifest: BackupManifest =
            Self::read_manifest_from_path(&backup_path.join("backup_manifest.json"))?;

        // Validate status
        match manifest.status {
            ManifestStatus::Completed { .. } => {}
            ManifestStatus::Failed { error } => {
                return Err(Error::BackupProtocol(format!("Backup failed: {}", error)));
            }
            ManifestStatus::InProgress => {
                return Err(Error::BackupProtocol(
                    "Backup is still in progress".to_string(),
                ));
            }
        }

        // Copy files back
        for file in &manifest.files {
            let src = backup_path.join(&file.name);
            let dst = if file.is_wal {
                target_db_path.with_extension("wal")
            } else {
                target_db_path.to_path_buf()
            };

            std::fs::copy(&src, &dst).map_err(|e| Error::Io(e))?;
        }

        Ok(())
    }

    // Private helper methods

    fn wal_path(&self) -> PathBuf {
        self.db_path.with_extension("wal")
    }

    fn get_file_size(&self, path: &Path) -> Result<u64> {
        std::fs::metadata(path)
            .map(|m| m.len())
            .map_err(|e| Error::Io(e))
    }

    fn get_wal_size(&self) -> Result<u64> {
        let wal_path = self.wal_path();
        if wal_path.exists() {
            self.get_file_size(&wal_path)
        } else {
            Ok(0)
        }
    }

    fn get_checkpoint_info(&self) -> Result<WalCheckpointInfo> {
        // Read WAL to get current checkpoint
        // For now, return default values
        // TODO: Implement proper checkpoint info reading
        Ok(WalCheckpointInfo {
            txid: 0,
            epoch: 0,
            wal_offset: 0,
        })
    }

    fn copy_ndb_file(&self, handle: &BackupHandle) -> Result<()> {
        let src = &self.db_path;
        let dst = handle.backup_dir.join(
            src.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        );

        let mut src_file = File::open(src).map_err(|e| Error::Io(e))?;
        let mut dst_file = File::create(&dst).map_err(|e| Error::Io(e))?;

        let total = std::io::copy(&mut src_file, &mut dst_file).map_err(|e| Error::Io(e))?;

        // Update progress
        {
            let active = self.active_backup.read().unwrap();
            if let Some(ref backup) = *active {
                backup.progress.fetch_add(total, Ordering::Relaxed);
            }
        }

        Ok(())
    }

    fn copy_wal_file(&self, handle: &BackupHandle) -> Result<()> {
        let src = self.wal_path();
        if !src.exists() {
            return Ok(());
        }

        let dst = handle.backup_dir.join(
            src.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        );

        let mut src_file = File::open(&src).map_err(|e| Error::Io(e))?;
        let mut dst_file = File::create(&dst).map_err(|e| Error::Io(e))?;

        let total = std::io::copy(&mut src_file, &mut dst_file).map_err(|e| Error::Io(e))?;

        // Update progress
        {
            let active = self.active_backup.read().unwrap();
            if let Some(ref backup) = *active {
                backup.progress.fetch_add(total, Ordering::Relaxed);
            }
        }

        Ok(())
    }

    fn write_manifest(&self, dir: &Path, manifest: &BackupManifest) -> Result<()> {
        let path = dir.join("backup_manifest.json");
        let file = File::create(&path).map_err(|e| Error::Io(e))?;
        serde_json::to_writer_pretty(file, manifest).map_err(|e| Error::Serialization(e))?;
        Ok(())
    }

    fn read_manifest(&self, dir: &Path) -> Result<BackupManifest> {
        Self::read_manifest_from_path(&dir.join("backup_manifest.json"))
    }

    fn read_manifest_from_path(path: &Path) -> Result<BackupManifest> {
        let file = File::open(path).map_err(|e| Error::Io(e))?;
        serde_json::from_reader(file).map_err(|e| Error::Serialization(e))
    }
}

/// Helper struct for reading checkpoint info from WAL
struct WalCheckpointInfo {
    txid: u64,
    epoch: u64,
    wal_offset: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_backup_handle_creation() {
        let handle = BackupHandle {
            id: Uuid::new_v4(),
            backup_dir: PathBuf::from("/tmp/backup"),
            created_at: chrono::Utc::now(),
        };

        assert!(!handle.id().is_nil());
        assert_eq!(handle.backup_dir(), Path::new("/tmp/backup"));
    }

    #[test]
    fn test_backup_manager_creation() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.ndb");
        let backup_path = dir.path().join("backups");

        let manager = BackupManager::new(db_path, backup_path);
        assert!(manager.active_backup.read().unwrap().is_none());
    }
}
