use std::collections::BTreeMap;

/// Ranked layers: a crate may depend on the same or a higher rank (deeper),
/// but must not depend on a lower rank (shallower / more UI-facing).
#[derive(Debug, Clone)]
pub struct LayerDoctrine {
    pub name: String,
    /// crate name → rank (0 = top/UI, larger = storage)
    pub crate_rank: BTreeMap<String, u32>,
}

impl LayerDoctrine {
    pub fn detect(crate_names: &[String]) -> Self {
        if crate_names
            .iter()
            .any(|c| c == "binder-ui" || c == "binder-app")
        {
            Self::binder()
        } else {
            Self::flat(crate_names)
        }
    }

    pub fn binder() -> Self {
        let mut crate_rank = BTreeMap::new();
        for c in ["binder-ui", "binder-app", "binder-home", "binder-commands"] {
            crate_rank.insert(c.to_string(), 0);
        }
        for c in [
            "binder-runtime",
            "binder-sdk",
            "binder-platform",
            "binder-services",
            "binder-service-rt",
            "binder-events",
            "binder-workspace",
            "binder-watcher",
            "binder-ingest",
            "binder-telemetry",
            "binder-ai",
            "binder-ai-mistralrs",
            "binder-binders",
            "binder-manifest",
        ] {
            crate_rank.insert(c.to_string(), 1);
        }
        for c in [
            "binder-dpk",
            "binder-core",
            "dpk-core",
            "dpk-query",
            "dpk-graph",
            "dpk-analytics",
            "dpk-semantic",
            "dpk-identity",
        ] {
            crate_rank.insert(c.to_string(), 2);
        }
        crate_rank.insert("binder-storage".into(), 3);
        Self {
            name: "binder".into(),
            crate_rank,
        }
    }

    pub fn flat(crate_names: &[String]) -> Self {
        let mut crate_rank = BTreeMap::new();
        for c in crate_names {
            crate_rank.insert(c.clone(), 0);
        }
        Self {
            name: "flat".into(),
            crate_rank,
        }
    }

    pub fn rank_of(&self, crate_name: &str) -> Option<u32> {
        self.crate_rank.get(crate_name).copied()
    }

    /// True when `from` depends upward (on a shallower layer).
    pub fn violates(&self, from_crate: &str, to_crate: &str) -> bool {
        match (self.rank_of(from_crate), self.rank_of(to_crate)) {
            (Some(f), Some(t)) => t < f,
            _ => false,
        }
    }
}
