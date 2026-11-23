//! Decides which SOCKS targets the listeners accept.
//!
//! Only `.onion` hostnames are accepted. IP addresses and ordinary hostnames
//! are refused before anything is sent to Tor.

use tor_socksproto::SocksAddr;

/// Returns the hostname if `addr` is an onion service hostname.
///
/// The check is deliberately loose about the address itself: Arti validates
/// the onion address when it connects. This function only guarantees that
/// nothing other than a `.onion` name gets that far.
pub(crate) fn onion_hostname(addr: &SocksAddr) -> Option<String> {
    match addr {
        SocksAddr::Hostname(name) => {
            let name: &str = name.as_ref();
            is_onion_hostname(name).then(|| name.to_ascii_lowercase())
        }
        SocksAddr::Ip(_) => None,
    }
}

/// True for `<label>.onion`, optionally with subdomains in front.
fn is_onion_hostname(name: &str) -> bool {
    let name = name.strip_suffix('.').unwrap_or(name);
    let Some((rest, tld)) = name.rsplit_once('.') else {
        return false;
    };
    if !tld.eq_ignore_ascii_case("onion") {
        return false;
    }
    let service_label = rest.rsplit('.').next().unwrap_or("");
    !service_label.is_empty()
        && rest
            .split('.')
            .all(|label| !label.is_empty() && label.bytes().all(is_label_byte))
}

fn is_label_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

#[cfg(test)]
mod tests {
    use super::*;

    const V3: &str = "2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid";

    #[test]
    fn accepts_onion_hostnames() {
        assert!(is_onion_hostname(&format!("{V3}.onion")));
        assert!(is_onion_hostname(&format!("{V3}.ONION")));
        assert!(is_onion_hostname(&format!("www.{V3}.onion")));
        assert!(is_onion_hostname(&format!("{V3}.onion.")));
    }

    #[test]
    fn refuses_everything_else() {
        for name in [
            "",
            "onion",
            ".onion",
            "example.com",
            "node.example.onion.com",
            "onion.example.com",
            "exa mple.onion",
            "a..onion",
            "127.0.0.1",
            "localhost",
        ] {
            assert!(!is_onion_hostname(name), "{name:?} must be refused");
        }
    }

    #[test]
    fn refuses_ip_addresses() {
        let v4 = SocksAddr::Ip("203.0.113.7".parse().unwrap());
        let v6 = SocksAddr::Ip("2001:db8::1".parse().unwrap());
        assert_eq!(onion_hostname(&v4), None);
        assert_eq!(onion_hostname(&v6), None);
    }

    #[test]
    fn lowercases_accepted_hostnames() {
        let addr = SocksAddr::Hostname(format!("{}.Onion", V3.to_uppercase()).try_into().unwrap());
        assert_eq!(onion_hostname(&addr), Some(format!("{V3}.onion")));
    }
}
