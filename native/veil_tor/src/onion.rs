//! Decides which SOCKS targets the listeners accept.
//!
//! Only `.onion` hostnames are accepted. IP addresses and ordinary hostnames
//! are refused before anything is sent to Tor.

use tor_socksproto::SocksAddr;

/// Returns the hostname if `addr` is an onion service hostname: in small
/// letters and without the dot a fully written name may end with.
///
/// The check is deliberately loose about the address itself: Arti validates
/// the onion address when it connects. This function only guarantees that
/// nothing other than a `.onion` name gets that far.
///
/// What is returned always ends in `.onion`, which is the one thing Arti
/// goes by when it decides between an onion service and a host to reach
/// through an exit relay. A name handed on with its last dot would not end
/// that way. The Arti in use refuses such a name outright, so it went
/// nowhere; that is Arti's rule to change, and this guarantee should not
/// hang on it.
pub(crate) fn onion_hostname(addr: &SocksAddr) -> Option<String> {
    match addr {
        SocksAddr::Hostname(name) => {
            let name: &str = name.as_ref();
            is_onion_hostname(name)
                .then(|| name.strip_suffix('.').unwrap_or(name).to_ascii_lowercase())
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
    fn the_last_dot_of_a_full_name_is_not_handed_on() {
        let addr = SocksAddr::Hostname(format!("{V3}.onion.").try_into().unwrap());
        assert_eq!(onion_hostname(&addr), Some(format!("{V3}.onion")));
    }

    /// Whatever is let through, Arti itself takes for an onion service or
    /// refuses: never for a host to reach through an exit relay, and never
    /// for an address.
    #[test]
    fn made_up_names_reach_arti_as_onion_services_or_not_at_all() {
        let plain = format!("{V3}.onion");
        let shouted = format!("{}.ONION", V3.to_uppercase());
        let under = format!("www.{V3}.onion");
        let full = format!("{V3}.onion.");
        let samples = [
            plain.as_str(),
            shouted.as_str(),
            under.as_str(),
            full.as_str(),
            "example.com",
            "node.example.onion.com",
            "onion.example.com",
            "127.0.0.1",
            "::1",
            "[2001:db8::1]",
            "localhost",
            "a.onion",
            "-.onion",
            "1.2.3.4.onion",
        ];
        veil_test_guard::fuzz::texts("onion name", &samples, |text| {
            // What cannot be a name in a SOCKS request never gets here.
            let Ok(name) = text.to_owned().try_into() else {
                return;
            };
            let Some(host) = onion_hostname(&SocksAddr::Hostname(name)) else {
                return;
            };
            assert!(host.ends_with(".onion"), "{host:?}");
            assert!(
                host.bytes().all(|b| b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || b == b'-'
                    || b == b'.'),
                "{host:?}"
            );
            assert!(host.split('.').all(|label| !label.is_empty()), "{host:?}");
            assert!(host.parse::<std::net::IpAddr>().is_err(), "{host:?}");
            // Arti's own reading of it. Its `Debug` output is not a promise
            // of Arti's; if it changes, this has to be asked another way.
            if let Ok(address) = arti_client::TorAddr::from((host.as_str(), 18081)) {
                let seen = format!("{address:?}");
                assert!(seen.contains("Onion("), "{seen}");
                assert!(!address.is_ip_address(), "{seen}");
            }
        });
    }

    #[test]
    fn lowercases_accepted_hostnames() {
        let addr = SocksAddr::Hostname(format!("{}.Onion", V3.to_uppercase()).try_into().unwrap());
        assert_eq!(onion_hostname(&addr), Some(format!("{V3}.onion")));
    }
}
