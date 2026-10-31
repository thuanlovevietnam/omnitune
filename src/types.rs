#[derive(Debug, Clone, PartialEq)]
pub enum Tab {
    Scenarios,
    Dashboard,
    Cleaner,
    Memory,
    Power,
    Protection,
}

#[derive(Debug, Clone)]
pub struct JunkCategory {
    pub name: String,
    pub path_desc: String,
    pub size_bytes: u64,
    pub file_count: usize,
    pub selected: bool,
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
    pub current_charge_percent: u8,
    pub wear_percent: f32,
    pub is_charging: bool,
}
