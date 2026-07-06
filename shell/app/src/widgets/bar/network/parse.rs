pub(super) fn parse_ssid(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Ok(String::new());
    }

    let bytes = if value.contains("U8(") {
        parse_owned_u8_array(value)?
    } else {
        value
            .split_whitespace()
            .map(|part| {
                part.parse::<u8>()
                    .map_err(|error| format!("invalid SSID byte {part}: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?
    };

    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn parse_owned_u8_array(value: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut rest = value;

    while let Some((_, after_prefix)) = rest.split_once("U8(") {
        let Some((byte, after_byte)) = after_prefix.split_once(')') else {
            return Err(format!("invalid U8 array value: {value}"));
        };
        bytes.push(
            byte.parse::<u8>()
                .map_err(|error| format!("invalid SSID byte {byte}: {error}"))?,
        );
        rest = after_byte;
    }

    Ok(bytes)
}
