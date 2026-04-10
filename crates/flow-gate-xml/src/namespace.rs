pub const NS_GATING: &str = "http://www.isac-net.org/std/Gating-ML/v2.0/gating";
pub const NS_TRANSFORMS: &str = "http://www.isac-net.org/std/Gating-ML/v2.0/transformations";
pub const NS_DATATYPE: &str = "http://www.isac-net.org/std/Gating-ML/v2.0/datatypes";

pub fn parse_bool_attr(value: Option<&str>, default: bool) -> bool {
    value
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "true" | "1"))
        .unwrap_or(default)
}
