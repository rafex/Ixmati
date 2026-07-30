pub fn parse_mqtt_broker(url: &str) -> (String, u16) {
    let cleaned = url.trim_start_matches("tcp://").trim_start_matches("mqtt://");

    if let Some((host, port_str)) = cleaned.rsplit_once(':') {
        if let Ok(port) = port_str.parse::<u16>() {
            return (host.to_string(), port);
        }
    }

    (cleaned.to_string(), 1883)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_url_with_port() {
        let (host, port) = parse_mqtt_broker("tcp://mosquitto:30200");
        assert_eq!(host, "mosquitto");
        assert_eq!(port, 30200);
    }

    #[test]
    fn parse_host_port_only() {
        let (host, port) = parse_mqtt_broker("localhost:1883");
        assert_eq!(host, "localhost");
        assert_eq!(port, 1883);
    }

    #[test]
    fn parse_host_default_port() {
        let (host, port) = parse_mqtt_broker("mosquitto");
        assert_eq!(host, "mosquitto");
        assert_eq!(port, 1883);
    }

    #[test]
    fn parse_tcp_without_port() {
        let (host, port) = parse_mqtt_broker("tcp://localhost");
        assert_eq!(host, "localhost");
        assert_eq!(port, 1883);
    }

    #[test]
    fn parse_mqtt_prefix() {
        let (host, port) = parse_mqtt_broker("mqtt://broker:1883");
        assert_eq!(host, "broker");
        assert_eq!(port, 1883);
    }
}
