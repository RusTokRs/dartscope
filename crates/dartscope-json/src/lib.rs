use serde::Serialize;

pub fn to_json_pretty<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(value)
}

pub fn to_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dartscope_core::DartFileAnalysis;

    #[test]
    fn serializes_analysis_as_pretty_json() {
        let json = to_json_pretty(&DartFileAnalysis::empty("lib/main.dart")).unwrap();

        assert!(json.contains("\"path\": \"lib/main.dart\""));
        assert!(json.contains("\"imports\": []"));
    }
}
