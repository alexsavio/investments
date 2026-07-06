// Some Russian services have to use certificates issued by The Ministry of Digital Development and Communications. We
// don't expect that this CA is added as trusted to the system, so bundle it here and trust it where we have to.

// Russian Trusted Root CA (https://www.gosuslugi.ru/crt)
// Not valid after: Saturday, 28 February 2032 at 00:04:15 Moscow Standard Time
pub const RUSSIAN_TRUSTED_ROOT_CA: &[u8] = include_bytes!("russian_trusted_root_ca.crt");