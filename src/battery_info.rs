use crate::types::BatteryDetails;
use serde::Deserialize;
use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Deserialize, Debug, Default)]
struct BatteryWmiBasic {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "EstimatedChargeRemaining")]
    estimated_charge_remaining: Option<u8>,
    #[serde(rename = "BatteryStatus")]
    battery_status: Option<u32>,
}

#[derive(Deserialize, Debug, Default)]
struct BatteryStatic {
    #[serde(rename = "DesignedCapacity")]
    designed_capacity: Option<u64>,
}

#[derive(Deserialize, Debug, Default)]
struct BatteryCycle {
    #[serde(rename = "CycleCount")]
    cycle_count: Option<u32>,
}

#[derive(Deserialize, Debug, Default)]
struct BatteryFullCap {
    #[serde(rename = "FullChargedCapacity")]
    full_charged_capacity: Option<u64>,
}

#[derive(Deserialize, Debug, Default)]
struct BatteryRealTimeStatus {
    #[serde(rename = "DischargeRate")]
    discharge_rate: Option<u32>,
    #[serde(rename = "ChargeRate")]
    charge_rate: Option<u32>,
    #[serde(rename = "Voltage")]
    voltage: Option<u32>,
    #[serde(rename = "RemainingCapacity")]
    remaining_capacity: Option<u64>,
}

#[derive(Deserialize, Debug, Default)]
struct BatteryCombinedPayload {
    #[serde(rename = "Basic")]
    basic: Option<serde_json::Value>,
    #[serde(rename = "Static")]
    r#static: Option<serde_json::Value>,
    #[serde(rename = "Cycles")]
    cycles: Option<serde_json::Value>,
    #[serde(rename = "Full")]
    full: Option<serde_json::Value>,
    #[serde(rename = "Status")]
    status: Option<serde_json::Value>,
}

pub fn get_dual_battery_info() -> Vec<BatteryDetails> {
    let script = r#"& {
$b = Get-CimInstance Win32_Battery | Select-Object Name, EstimatedChargeRemaining, BatteryStatus;
$s = Get-WmiObject -Namespace root/wmi -Class BatteryStaticData | Select-Object DesignedCapacity;
$c = Get-WmiObject -Namespace root/wmi -Class BatteryCycleCount | Select-Object CycleCount;
$f = Get-WmiObject -Namespace root/wmi -Class BatteryFullChargedCapacity | Select-Object FullChargedCapacity;
$t = Get-WmiObject -Namespace root/wmi -Class BatteryStatus | Select-Object DischargeRate, ChargeRate, Voltage, RemainingCapacity;
ConvertTo-Json @{ Basic=$b; Static=$s; Cycles=$c; Full=$f; Status=$t }
}"#;

    let json_text = run_ps_script(script);
    if json_text.trim().is_empty() {
        return Vec::new();
    }

    let payload: BatteryCombinedPayload = match serde_json::from_str(&json_text) {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };

    let basic_list: Vec<BatteryWmiBasic> = parse_value_list(payload.basic);
    let static_list: Vec<BatteryStatic> = parse_value_list(payload.r#static);
    let cycles_list: Vec<BatteryCycle> = parse_value_list(payload.cycles);
    let full_list: Vec<BatteryFullCap> = parse_value_list(payload.full);
    let status_list: Vec<BatteryRealTimeStatus> = parse_value_list(payload.status);

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

        let r_status = status_list.get(i);
        let discharge_rate = r_status.and_then(|s| s.discharge_rate).unwrap_or(0);
        let charge_rate = r_status.and_then(|s| s.charge_rate).unwrap_or(0);
        let voltage = r_status.and_then(|s| s.voltage).unwrap_or(0);
        let remaining_cap = r_status.and_then(|s| s.remaining_capacity).unwrap_or(0);

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
            remaining_capacity_mwh: remaining_cap,
            current_charge_percent: charge,
            wear_percent: wear,
            is_charging,
            discharge_rate_mw: discharge_rate,
            charge_rate_mw: charge_rate,
            voltage_mv: voltage,
        });
    }

    result
}

fn run_ps_script(script: &str) -> String {
    let output = Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    output
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

fn parse_value_list<T: for<'de> Deserialize<'de>>(val: Option<serde_json::Value>) -> Vec<T> {
    let Some(v) = val else {
        return Vec::new();
    };

    if v.is_array() {
        serde_json::from_value(v).unwrap_or_default()
    } else if v.is_object() {
        if let Ok(single) = serde_json::from_value::<T>(v) {
            vec![single]
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    }
}
