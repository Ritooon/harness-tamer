//! Description de la machine (mémoire, cœurs, système) et recommandation de
//! modèle. Tout est lu localement ; rien n'est envoyé.

use serde::{Deserialize, Serialize};

use super::catalog::{Catalog, ModelRole};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct HardwareInfo {
    pub os: String,
    pub arch: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total_memory_bytes: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub available_memory_bytes: u64,
    pub cpu_threads: u32,
    /// Mac Apple Silicon : mémoire unifiée, utilisable par le GPU (Metal).
    pub unified_memory: bool,
}

impl HardwareInfo {
    pub fn detect() -> Self {
        // Si la lecture échoue (plateforme exotique), la mémoire vaut 0 et
        // l'interface propose le plus petit modèle.
        let (total, available) = match sysinfo::System::new() {
            Ok(mut sys) => {
                sys.refresh_memory();
                (sys.total_memory(), sys.available_memory())
            }
            Err(_) => (0, 0),
        };
        let os = std::env::consts::OS.to_owned();
        let arch = std::env::consts::ARCH.to_owned();
        Self {
            unified_memory: os == "macos" && arch == "aarch64",
            os,
            arch,
            total_memory_bytes: total,
            available_memory_bytes: available,
            cpu_threads: std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1),
        }
    }

    pub fn total_memory_gb(&self) -> f64 {
        self.total_memory_bytes as f64 / 1024f64.powi(3)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Recommendation {
    pub model_id: String,
    /// `true` si même le plus petit modèle dépasse la mémoire conseillée.
    pub below_minimum: bool,
}

/// Premier modèle (par ordre de préférence) dont la mémoire conseillée tient
/// dans la RAM de la machine. La VRAM d'un GPU dédié n'est pas détectée de
/// façon fiable sur les trois OS : on raisonne sur la RAM, ce qui reste
/// prudent (llama.cpp répartit ensuite entre GPU et CPU).
pub fn recommend(hw: &HardwareInfo, catalog: &Catalog) -> Recommendation {
    let ram_gb = hw.total_memory_gb();
    let order = [ModelRole::Default, ModelRole::Light, ModelRole::Small];
    let mut candidates: Vec<_> =
        order.iter().flat_map(|role| catalog.models.iter().filter(move |m| m.role == *role)).collect();
    // Parmi les petits modèles, le plus gros qui tient.
    candidates.sort_by_key(|m| (order.iter().position(|r| *r == m.role), std::cmp::Reverse(m.min_memory_gb)));
    if let Some(m) = candidates.iter().find(|m| f64::from(m.min_memory_gb) <= ram_gb + 0.5) {
        return Recommendation { model_id: m.id.clone(), below_minimum: false };
    }
    let smallest = catalog.models.iter().min_by_key(|m| m.min_memory_gb).expect("catalog is not empty");
    Recommendation { model_id: smallest.id.clone(), below_minimum: true }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hw(gb: u64) -> HardwareInfo {
        HardwareInfo {
            os: "linux".into(),
            arch: "x86_64".into(),
            total_memory_bytes: gb * 1024 * 1024 * 1024,
            available_memory_bytes: 0,
            cpu_threads: 8,
            unified_memory: false,
        }
    }

    #[test]
    fn recommendation_follows_memory() {
        let c = Catalog::builtin();
        assert_eq!(recommend(&hw(64), &c).model_id, "qwen3.6-35b-a3b-q4km");
        // 31,7 Go affichés pour une barrette de 32 Go : on garde le modèle par défaut.
        let mut almost = hw(32);
        almost.total_memory_bytes -= 300 * 1024 * 1024;
        assert_eq!(recommend(&almost, &c).model_id, "qwen3.6-35b-a3b-q4km");
        assert_eq!(recommend(&hw(16), &c).model_id, "qwen3.5-9b-q4km");
        assert_eq!(recommend(&hw(8), &c).model_id, "gemma-4-e4b-q4km");
        let tiny = recommend(&hw(4), &c);
        assert_eq!((tiny.model_id.as_str(), tiny.below_minimum), ("qwen3.5-4b-q4km", true));
    }

    #[test]
    fn detection_returns_plausible_values() {
        let h = HardwareInfo::detect();
        assert!(h.total_memory_bytes > 0);
        assert!(h.cpu_threads >= 1);
    }
}
