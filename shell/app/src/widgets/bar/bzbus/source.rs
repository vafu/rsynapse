use shell_core::source::{self, Observable};

use super::view::{self, BzBusView};

pub(in crate::widgets::bar) fn bzbus_status() -> Observable<BzBusView> {
    source::once(view::view(false, Vec::new()))
}

#[cfg(test)]
pub(super) fn parse_i64(value: &str) -> i64 {
    parse_wrapped_integer(value)
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0)
}

#[cfg(test)]
pub(super) fn parse_u32(value: &str) -> u32 {
    parse_wrapped_integer(value)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
}

#[cfg(test)]
fn parse_wrapped_integer(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(wrapped) = value
        .strip_prefix("OwnedValue(I64(")
        .and_then(|value| value.strip_suffix("))"))
        .or_else(|| {
            value
                .strip_prefix("OwnedValue(U32(")
                .and_then(|value| value.strip_suffix("))"))
        })
    {
        return Some(wrapped);
    }
    Some(value)
}
