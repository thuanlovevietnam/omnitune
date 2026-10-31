#[derive(Debug, Clone, PartialEq)]
pub enum Tab {
    Scenarios,
    Criteria,
    Dashboard,
    Cleaner,
    Memory,
    Power,
    Protection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanTier {
    Safe,
    Developer,
    SystemMaintenance,
}

#[derive(Debug, Clone)]
pub struct JunkCategory {
    pub name: String,
    pub path_desc: String,
    pub size_bytes: u64,
    pub file_count: usize,
    pub selected: bool,
    pub tier: CleanTier,
}

#[derive(Debug, Clone)]
pub struct ProcessMemInfo {
    pub pid: u32,
    pub name: String,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct BatteryDetails {
    pub id: String,
    pub cycle_count: u32,
    pub design_capacity_mwh: u64,
    pub full_charge_capacity_mwh: u64,
    pub remaining_capacity_mwh: u64,
    pub current_charge_percent: u8,
    pub wear_percent: f32,
    pub is_charging: bool,
    pub discharge_rate_mw: u32,
    pub charge_rate_mw: u32,
    pub voltage_mv: u32,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct StartupAppItem {
    pub name: String,
    pub command: String,
    pub location: String,
    pub user: String,
    pub is_enabled: bool,
}
