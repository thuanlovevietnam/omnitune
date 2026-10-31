use crate::types::BatteryDetails;
use serde::Deserialize;
use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Deserialize, Debug)]
struct BatteryWmiBasic {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "EstimatedChargeRemaining")]
    estimated_charge_remaining: Option<u8>,
    #[serde(rename = "BatteryStatus")]
    battery_status: Option<u32>,
}

#[derive(Deserialize, Debug)]
struct BatteryStatic {
    #[serde(rename = "DesignedCapacity")]
    designed_capacity: Option<u64>,
}

#[derive(Deserialize, Debug)]
struct BatteryCycle {
    #[serde(rename = "CycleCount")]
    cycle_count: Option<u32>,
}

#[derive(Deserialize, Debug)]
struct BatteryFullCap {
    #[serde(rename = "FullChargedCapacity")]
    full_charged_capacity: Option<u64>,
}

pub fn get_dual_battery_info() -> Vec<BatteryDetails> {
    // 1. Basic battery status and percentage from Win32_Battery
    let ps_basic = "Get-CimInstance Win32_Battery | Select-Object Name, EstimatedChargeRemaining, BatteryStatus | ConvertTo-Json";
    let basic_json = run_ps_json(ps_basic);
    let basic_list: Vec<BatteryWmiBasic> = parse_json_array(&basic_json);

    // 2. Real Design Capacity from ACPI BatteryStaticData
    let ps_static = "Get-WmiObject -Namespace root/wmi -Class BatteryStaticData | Select-Object DesignedCapacity | ConvertTo-Json";
    let static_json = run_ps_json(ps_static);
    let static_list: Vec<BatteryStatic> = parse_json_array(&static_json);

    // 3. Real Cycle Count from ACPI BatteryCycleCount
    let ps_cycles = "Get-WmiObject -Namespace root/wmi -Class BatteryCycleCount | Select-Object CycleCount | ConvertTo-Json";
    let cycles_json = run_ps_json(ps_cycles);
    let cycles_list: Vec<BatteryCycle> = parse_json_array(&cycles_json);

    // 4. Real Full Charge Capacity from ACPI BatteryFullChargedCapacity
    let ps_full = "Get-WmiObject -Namespace root/wmi -Class BatteryFullChargedCapacity | Select-Object FullChargedCapacity | ConvertTo-Json";
    let full_json = run_ps_json(ps_full);
    let full_list: Vec<BatteryFullCap> = parse_json_array(&full_json);

    let mut result = Vec::new();

    for (i, basic) in basic_list.into_iter().enumerate() {
        let name = basic.name.unwrap_or_else(|| format!("Pin {}", i + 1));
        let charge = basic.estimated_charge_remaining.unwrap_or(0);
        let status = basic.battery_status.unwrap_or(1);
        let is_charging = status == 2;

        let design_cap = static_list
            .get(i)
            .and_then(|s| s.designed_capacity)
            .unwrap_or(0);

        let full_cap = full_list
            .get(i)
            .and_then(|f| f.full_charged_capacity)
            .unwrap_or(0);

        let cycles = cycles_list
            .get(i)
            .and_then(|c| c.cycle_count)
            .unwrap_or(0);

        let wear = if design_cap > 0 && full_cap > 0 && full_cap <= design_cap {
            ((1.0 - (full_cap as f32 / design_cap as f32)) * 100.0).max(0.0)
        } else {
            0.0
        };

        result.push(BatteryDetails {
            id: name,
            cycle_count: cycles,
            design_capacity_mwh: design_cap,
            full_charge_capacity_mwh: full_cap,
            current_charge_percent: charge,
            wear_percent: wear,
            is_charging,
        });
    }

    result
}

fn run_ps_json(script: &str) -> String {
    let output = Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    output
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

fn parse_json_array<T: for<'de> Deserialize<'de>>(json: &str) -> Vec<T> {
    let trimmed = json.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    if trimmed.starts_with('[') {
        serde_json::from_str(trimmed).unwrap_or_default()
    } else if trimmed.starts_with('{') {
        if let Ok(single) = serde_json::from_str::<T>(trimmed) {
            vec![single]
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    }
}
