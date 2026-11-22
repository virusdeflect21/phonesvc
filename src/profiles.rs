//! Built-in device profiles (sector size + an illustrative partition layout).
//!
//! Real sector sizes and, especially, partition layouts vary by model and
//! firmware. These profiles are *starting points* only: the authoritative
//! layout for a given device always comes from that device's own firmware
//! (`rawprogram*.xml` for Qualcomm, the GPT on the device, or a PIT dump for
//! Samsung). Do not flash based on these tables alone.
//!
//! Storage classes:
//!   * eMMC  -> 512-byte sectors
//!   * UFS   -> 4096-byte sectors
//!   * NVMe  -> 512/4096-byte sectors (rare on Qualcomm phones)

#[derive(Debug, Clone, Copy)]
pub struct PartitionProfile {
    pub label: &'static str,
    pub sector_size: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct DeviceProfile {
    pub name: &'static str,
    pub description: &'static str,
    pub vendor: Vendor,
    pub storage: Storage,
    pub sector_size: u32,
    pub partitions: &'static [PartitionProfile],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vendor {
    Qualcomm,
    Samsung,
    Google,
    Xiaomi,
    OnePlus,
    Oplus,
    Vivo,
    Oppo,
    Motorola,
    Lenovo,
    Asus,
    Sony,
    Lg,
    Htc,
    Microsoft,
    Fairphone,
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Storage {
    Emmc,
    Ufs,
    Nvme,
    Unknown,
}

// ---- partition templates (illustrative) ----

const EMMC_MIN: &[PartitionProfile] = &[
    PartitionProfile { label: "sbl1", sector_size: 512 },
    PartitionProfile { label: "aboot", sector_size: 512 },
    PartitionProfile { label: "modem", sector_size: 512 },
    PartitionProfile { label: "boot", sector_size: 512 },
    PartitionProfile { label: "recovery", sector_size: 512 },
    PartitionProfile { label: "system", sector_size: 512 },
    PartitionProfile { label: "cache", sector_size: 512 },
    PartitionProfile { label: "userdata", sector_size: 512 },
];

const EMMC_AB: &[PartitionProfile] = &[
    PartitionProfile { label: "xbl", sector_size: 512 },
    PartitionProfile { label: "abl", sector_size: 512 },
    PartitionProfile { label: "modem", sector_size: 512 },
    PartitionProfile { label: "boot_a", sector_size: 512 },
    PartitionProfile { label: "boot_b", sector_size: 512 },
    PartitionProfile { label: "dtbo_a", sector_size: 512 },
    PartitionProfile { label: "dtbo_b", sector_size: 512 },
    PartitionProfile { label: "vbmeta", sector_size: 512 },
    PartitionProfile { label: "system_a", sector_size: 512 },
    PartitionProfile { label: "system_b", sector_size: 512 },
    PartitionProfile { label: "vendor", sector_size: 512 },
    PartitionProfile { label: "product", sector_size: 512 },
    PartitionProfile { label: "userdata", sector_size: 512 },
];

const UFS_AB: &[PartitionProfile] = &[
    PartitionProfile { label: "xbl", sector_size: 4096 },
    PartitionProfile { label: "abl", sector_size: 4096 },
    PartitionProfile { label: "modem", sector_size: 4096 },
    PartitionProfile { label: "boot_a", sector_size: 4096 },
    PartitionProfile { label: "boot_b", sector_size: 4096 },
    PartitionProfile { label: "dtbo_a", sector_size: 4096 },
    PartitionProfile { label: "dtbo_b", sector_size: 4096 },
    PartitionProfile { label: "vbmeta", sector_size: 4096 },
    PartitionProfile { label: "super", sector_size: 4096 },
    PartitionProfile { label: "userdata", sector_size: 4096 },
];

const UFS_LEGACY: &[PartitionProfile] = &[
    PartitionProfile { label: "sbl1", sector_size: 4096 },
    PartitionProfile { label: "aboot", sector_size: 4096 },
    PartitionProfile { label: "modem", sector_size: 4096 },
    PartitionProfile { label: "boot", sector_size: 4096 },
    PartitionProfile { label: "recovery", sector_size: 4096 },
    PartitionProfile { label: "system", sector_size: 4096 },
    PartitionProfile { label: "cache", sector_size: 4096 },
    PartitionProfile { label: "userdata", sector_size: 4096 },
];

const SAMSUNG_EMMC: &[PartitionProfile] = &[
    PartitionProfile { label: "sboot", sector_size: 512 },
    PartitionProfile { label: "boot", sector_size: 512 },
    PartitionProfile { label: "recovery", sector_size: 512 },
    PartitionProfile { label: "system", sector_size: 512 },
    PartitionProfile { label: "hidden", sector_size: 512 },
    PartitionProfile { label: "userdata", sector_size: 512 },
];

const SAMSUNG_UFS: &[PartitionProfile] = &[
    PartitionProfile { label: "sboot", sector_size: 4096 },
    PartitionProfile { label: "boot", sector_size: 4096 },
    PartitionProfile { label: "vendor", sector_size: 4096 },
    PartitionProfile { label: "system", sector_size: 4096 },
    PartitionProfile { label: "super", sector_size: 4096 },
    PartitionProfile { label: "userdata", sector_size: 4096 },
];

pub static BUILTIN: &[DeviceProfile] = &[
    // ---- generic fallbacks ----
    DeviceProfile {
        name: "generic-emmc-512",
        description: "Generic eMMC device (512-byte sectors)",
        vendor: Vendor::Generic,
        storage: Storage::Emmc,
        sector_size: 512,
        partitions: EMMC_MIN,
    },
    DeviceProfile {
        name: "generic-ufs-4096",
        description: "Generic UFS device (4096-byte sectors)",
        vendor: Vendor::Generic,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },

    // ---- Qualcomm chipset families ----
    DeviceProfile {
        name: "qcom-msm8916",
        description: "Qualcomm Snapdragon 410 (MSM8916), eMMC",
        vendor: Vendor::Qualcomm,
        storage: Storage::Emmc,
        sector_size: 512,
        partitions: EMMC_MIN,
    },
    DeviceProfile {
        name: "qcom-msm8953",
        description: "Qualcomm Snapdragon 625 (MSM8953), eMMC",
        vendor: Vendor::Qualcomm,
        storage: Storage::Emmc,
        sector_size: 512,
        partitions: EMMC_AB,
    },
    DeviceProfile {
        name: "qcom-msm8998",
        description: "Qualcomm Snapdragon 835 (MSM8998), UFS",
        vendor: Vendor::Qualcomm,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_LEGACY,
    },
    DeviceProfile {
        name: "qcom-sdm845",
        description: "Qualcomm Snapdragon 845 (SDM845), UFS",
        vendor: Vendor::Qualcomm,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "qcom-sm8150",
        description: "Qualcomm Snapdragon 855 (SM8150), UFS",
        vendor: Vendor::Qualcomm,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "qcom-sm8250",
        description: "Qualcomm Snapdragon 865/870 (SM8250), UFS",
        vendor: Vendor::Qualcomm,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "qcom-sm8350",
        description: "Qualcomm Snapdragon 888 (SM8350), UFS",
        vendor: Vendor::Qualcomm,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "qcom-sm8450",
        description: "Qualcomm Snapdragon 8 Gen 1 (SM8450), UFS",
        vendor: Vendor::Qualcomm,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "qcom-sm8550",
        description: "Qualcomm Snapdragon 8 Gen 2 (SM8550), UFS",
        vendor: Vendor::Qualcomm,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "qcom-sm8650",
        description: "Qualcomm Snapdragon 8 Gen 3 (SM8650), UFS",
        vendor: Vendor::Qualcomm,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "qcom-qcm6490",
        description: "Qualcomm QCM6490 (IoT), UFS",
        vendor: Vendor::Qualcomm,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },

    // ---- Google Pixel ----
    DeviceProfile {
        name: "google-pixel-3",
        description: "Google Pixel 3 (SDM845), UFS",
        vendor: Vendor::Google,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "google-pixel-4",
        description: "Google Pixel 4 (SM8150), UFS",
        vendor: Vendor::Google,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "google-pixel-5",
        description: "Google Pixel 5 (SM7250), UFS",
        vendor: Vendor::Google,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "google-pixel-6",
        description: "Google Pixel 6 (Tensor GS101), UFS",
        vendor: Vendor::Google,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "google-pixel-7",
        description: "Google Pixel 7 (Tensor G2), UFS",
        vendor: Vendor::Google,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },

    // ---- Samsung ----
    DeviceProfile {
        name: "samsung-galaxy-s8",
        description: "Samsung Galaxy S8 (Exynos 8895), UFS",
        vendor: Vendor::Samsung,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: SAMSUNG_UFS,
    },
    DeviceProfile {
        name: "samsung-galaxy-s9",
        description: "Samsung Galaxy S9 (Exynos 9810), UFS",
        vendor: Vendor::Samsung,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: SAMSUNG_UFS,
    },
    DeviceProfile {
        name: "samsung-galaxy-s10",
        description: "Samsung Galaxy S10 (Exynos 9820), UFS",
        vendor: Vendor::Samsung,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: SAMSUNG_UFS,
    },
    DeviceProfile {
        name: "samsung-galaxy-a50",
        description: "Samsung Galaxy A50 (Exynos 9610), UFS",
        vendor: Vendor::Samsung,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: SAMSUNG_UFS,
    },
    DeviceProfile {
        name: "samsung-galaxy-s7-emmc",
        description: "Samsung Galaxy S7 (Exynos 8890), eMMC",
        vendor: Vendor::Samsung,
        storage: Storage::Emmc,
        sector_size: 512,
        partitions: SAMSUNG_EMMC,
    },

    // ---- Xiaomi / Redmi / POCO ----
    DeviceProfile {
        name: "xiaomi-mi-8",
        description: "Xiaomi Mi 8 (SDM845), UFS",
        vendor: Vendor::Xiaomi,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "xiaomi-redmi-note-8",
        description: "Xiaomi Redmi Note 8 (SDM665), eMMC",
        vendor: Vendor::Xiaomi,
        storage: Storage::Emmc,
        sector_size: 512,
        partitions: EMMC_AB,
    },
    DeviceProfile {
        name: "xiaomi-poco-f1",
        description: "Xiaomi POCO F1 (SDM845), UFS",
        vendor: Vendor::Xiaomi,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "xiaomi-mi-11",
        description: "Xiaomi Mi 11 (SM8350), UFS",
        vendor: Vendor::Xiaomi,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },

    // ---- OnePlus / OPPO / realme ----
    DeviceProfile {
        name: "oneplus-6",
        description: "OnePlus 6 (SDM845), UFS",
        vendor: Vendor::OnePlus,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "oneplus-7",
        description: "OnePlus 7 (SM8150), UFS",
        vendor: Vendor::OnePlus,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "oneplus-8",
        description: "OnePlus 8 (SM8250), UFS",
        vendor: Vendor::OnePlus,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "oppo-find-x2",
        description: "OPPO Find X2 (SM8250), UFS",
        vendor: Vendor::Oppo,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },

    // ---- Vivo / iQOO ----
    DeviceProfile {
        name: "vivo-x50",
        description: "vivo X50 (SDM765G), UFS",
        vendor: Vendor::Vivo,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "vivo-iqoo-7",
        description: "iQOO 7 (SM8350), UFS",
        vendor: Vendor::Vivo,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },

    // ---- Motorola / Lenovo ----
    DeviceProfile {
        name: "motorola-moto-g7",
        description: "Motorola Moto G7 (SDM632), eMMC",
        vendor: Vendor::Motorola,
        storage: Storage::Emmc,
        sector_size: 512,
        partitions: EMMC_AB,
    },
    DeviceProfile {
        name: "motorola-edge-plus",
        description: "Motorola Edge+ (SM8250), UFS",
        vendor: Vendor::Motorola,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "lenovo-legion-phone-2",
        description: "Lenovo Legion Phone Duel 2 (SM8350), UFS",
        vendor: Vendor::Lenovo,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },

    // ---- ASUS / Sony / others ----
    DeviceProfile {
        name: "asus-zenfone-6",
        description: "ASUS ZenFone 6 (SM8150), UFS",
        vendor: Vendor::Asus,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "sony-xperia-1",
        description: "Sony Xperia 1 (SM8150), UFS",
        vendor: Vendor::Sony,
        storage: Storage::Ufs,
        sector_size: 4096,
        partitions: UFS_AB,
    },
    DeviceProfile {
        name: "fairphone-3",
        description: "Fairphone 3 (SDM632), eMMC",
        vendor: Vendor::Fairphone,
        storage: Storage::Emmc,
        sector_size: 512,
        partitions: EMMC_AB,
    },
];

pub fn find(name: &str) -> Option<&'static DeviceProfile> {
    BUILTIN.iter().find(|p| p.name == name)
}

pub fn by_vendor(v: Vendor) -> Vec<&'static DeviceProfile> {
    BUILTIN.iter().filter(|p| p.vendor == v).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_present_and_unique() {
        assert!(BUILTIN.len() >= 30);
        let mut names: Vec<&str> = BUILTIN.iter().map(|p| p.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate profile names");
    }

    #[test]
    fn sector_size_matches_storage() {
        for p in BUILTIN {
            match p.storage {
                Storage::Emmc => assert_eq!(p.sector_size, 512, "{}", p.name),
                Storage::Ufs => assert_eq!(p.sector_size, 4096, "{}", p.name),
                _ => {}
            }
        }
    }

    #[test]
    fn find_and_filter() {
        assert!(find("qcom-sm8250").is_some());
        assert!(find("does-not-exist").is_none());
        let qcom = by_vendor(Vendor::Qualcomm);
        assert!(qcom.iter().all(|p| p.vendor == Vendor::Qualcomm));
        assert!(qcom.len() >= 10);
    }
}