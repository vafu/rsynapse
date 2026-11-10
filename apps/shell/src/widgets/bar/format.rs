use super::*;

pub(super) fn battery_tooltip(battery: &BatteryView) -> String {
    format!("{}%", battery.percent)
}

pub(super) fn battery_icon_name(battery: &BatteryView) -> String {
    if !battery.present {
        return "battery-missing-symbolic".to_owned();
    }

    let charging = battery.state.is_charging();

    if battery.percent >= 95 && charging {
        return "battery-level-100-charged-symbolic".to_owned();
    }

    let level = (((battery.percent.min(100) as u16) + 5) / 10) * 10;
    let state = if charging { "-charging" } else { "" };

    format!("battery-level-{level}{state}-symbolic")
}

pub(super) fn source_error_tooltip(count: u64) -> String {
    format!("{count} source error(s) caught")
}

pub(super) fn source_error_count_label(count: u64) -> String {
    count.to_string()
}
