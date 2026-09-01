use crate::error::DomainError;
use crate::room::RoomKey;

const MAX_CLUSTER_K: u32 = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClusterExecution {
    DryRun,
    Execute,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClusterFreshnessPolicy {
    Always,
    IfStale,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClusterMaintenanceRequest {
    Check {
        room: RoomKey,
        k: u32,
    },
    Rebuild {
        room: RoomKey,
        execution: ClusterExecution,
        freshness: ClusterFreshnessPolicy,
        k: u32,
    },
}

impl ClusterMaintenanceRequest {
    pub fn check(room: RoomKey, k: u32) -> Result<Self, DomainError> {
        Self::validate_k(k)?;
        Ok(Self::Check { room, k })
    }

    pub fn rebuild(
        room: RoomKey,
        execution: ClusterExecution,
        freshness: ClusterFreshnessPolicy,
        k: u32,
    ) -> Result<Self, DomainError> {
        Self::validate_k(k)?;
        Ok(Self::Rebuild {
            room,
            execution,
            freshness,
            k,
        })
    }

    fn validate_k(k: u32) -> Result<(), DomainError> {
        if k == 0 || k > MAX_CLUSTER_K {
            return Err(DomainError::InvalidClusterMaintenance {
                field: "k".into(),
                message: format!("must be between 1 and {MAX_CLUSTER_K}"),
            });
        }
        Ok(())
    }

    pub fn room(&self) -> &RoomKey {
        match self {
            Self::Check { room, .. } | Self::Rebuild { room, .. } => room,
        }
    }

    pub const fn k(&self) -> u32 {
        match self {
            Self::Check { k, .. } | Self::Rebuild { k, .. } => *k,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClusterStaleness {
    built_at: Option<String>,
    clusters: u64,
    chunks_total: u64,
    chunks_since_build: u64,
    fraction_unseen: f64,
}

impl ClusterStaleness {
    pub fn new(
        built_at: Option<String>,
        clusters: u64,
        chunks_total: u64,
        chunks_since_build: u64,
        fraction_unseen: f64,
    ) -> Result<Self, DomainError> {
        if !fraction_unseen.is_finite() || !(0.0..=1.0).contains(&fraction_unseen) {
            return Err(DomainError::InvalidClusterMaintenance {
                field: "fractionUnseen".into(),
                message: "must be finite and between 0 and 1".into(),
            });
        }
        Ok(Self {
            built_at,
            clusters,
            chunks_total,
            chunks_since_build,
            fraction_unseen,
        })
    }
    pub fn built_at(&self) -> Option<&str> {
        self.built_at.as_deref()
    }
    pub const fn clusters(&self) -> u64 {
        self.clusters
    }
    pub const fn chunks_total(&self) -> u64 {
        self.chunks_total
    }
    pub const fn chunks_since_build(&self) -> u64 {
        self.chunks_since_build
    }
    pub const fn fraction_unseen(&self) -> f64 {
        self.fraction_unseen
    }
    pub const fn is_stale(&self, age_days: u64) -> bool {
        self.built_at.is_none()
            || (self.chunks_since_build > 0
                && (self.fraction_unseen >= 0.05
                    || self.chunks_since_build >= 250
                    || age_days >= 7))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClusterSummary {
    cluster_id: i64,
    label: String,
    member_count: u64,
    accepted: bool,
}

impl ClusterSummary {
    pub fn new(
        cluster_id: i64,
        label: impl Into<String>,
        member_count: u64,
        accepted: bool,
    ) -> Result<Self, DomainError> {
        let label = label.into();
        if label.trim().is_empty() {
            return Err(DomainError::InvalidClusterMaintenance {
                field: "label".into(),
                message: "must not be empty".into(),
            });
        }
        Ok(Self {
            cluster_id,
            label,
            member_count,
            accepted,
        })
    }
    pub const fn cluster_id(&self) -> i64 {
        self.cluster_id
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub const fn member_count(&self) -> u64 {
        self.member_count
    }
    pub const fn accepted(&self) -> bool {
        self.accepted
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClusterMaintenanceStatus {
    pub stale: bool,
    pub reason: String,
    pub staleness: ClusterStaleness,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClusterMaintenanceOutcome {
    Checked {
        status: ClusterMaintenanceStatus,
        clusters: Vec<ClusterSummary>,
    },
    SkippedFresh {
        status: ClusterMaintenanceStatus,
        clusters: Vec<ClusterSummary>,
    },
    DryRun {
        status: ClusterMaintenanceStatus,
    },
    Rebuilt {
        status: ClusterMaintenanceStatus,
        clusters: Vec<ClusterSummary>,
    },
}
