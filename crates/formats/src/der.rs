use serde::Serialize;

pub const NON_MINIMAL_LENGTH_NOTE: &str = "An element uses a non-minimal ASN.1 length; not strict DER.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tlv {
    pub tag: u8,
    pub start: usize,
    pub content: usize,
    pub len: usize,
    pub non_minimal_length: bool,
}

impl Tlv {
    pub fn end(&self) -> usize {
        self.content.saturating_add(self.len)
    }

    pub fn bytes<'a>(&self, data: &'a [u8]) -> Option<&'a [u8]> {
        data.get(self.content..self.end())
    }

    pub fn whole<'a>(&self, data: &'a [u8]) -> Option<&'a [u8]> {
        data.get(self.start..self.end())
    }

    pub fn constructed(&self) -> bool {
        self.tag & 0x20 != 0
    }
}

pub fn read(data: &[u8], off: usize) -> Option<Tlv> {
    let slice = data.get(off..)?;
    let (tlv, _) = craft_pkcs7::der::Tlv::parse_ber(slice).ok()?;
    let total_len = tlv.raw.len();
    let content_len = tlv.value.len();
    let hdr_len = total_len.checked_sub(content_len)?;
    let content = off.checked_add(hdr_len)?;
    let non_minimal_length = craft_pkcs7::der::Tlv::parse(slice).is_err();
    Some(Tlv {
        tag: tlv.tag,
        start: off,
        content,
        len: content_len,
        non_minimal_length,
    })
}

pub fn children(data: &[u8], parent: &Tlv) -> Vec<Tlv> {
    if !parent.constructed() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut off = parent.content;
    let end = parent.end();
    while off < end {
        let Some(child) = read(data, off) else { break };
        if child.end() > end {
            break;
        }
        off = child.end();
        out.push(child);
    }
    out
}

pub fn oid(bytes: &[u8]) -> String {
    let encoded = craft_pkcs7::der::tlv(craft_pkcs7::der::tag::OID, bytes);
    craft_pkcs7::der::Tlv::parse_all(&encoded)
        .and_then(|t| t.oid())
        .unwrap_or_else(|_| "invalid".to_string())
}

pub fn oid_name(oid: &str) -> Option<&'static str> {
    Some(match oid {
        "1.2.840.113549.1.1.1" => "rsaEncryption",
        "1.2.840.113549.1.1.5" => "sha1WithRSAEncryption",
        "1.2.840.113549.1.1.10" => "rsassaPss",
        "1.2.840.113549.1.1.11" => "sha256WithRSAEncryption",
        "1.2.840.113549.1.1.12" => "sha384WithRSAEncryption",
        "1.2.840.113549.1.1.13" => "sha512WithRSAEncryption",
        "1.2.840.10045.2.1" => "ecPublicKey",
        "1.2.840.10045.4.3.2" => "ecdsa-with-SHA256",
        "1.2.840.10045.4.3.3" => "ecdsa-with-SHA384",
        "1.2.840.10045.4.3.4" => "ecdsa-with-SHA512",
        "1.3.14.3.2.26" => "sha1",
        "2.16.840.1.101.3.4.2.1" => "sha256",
        "2.16.840.1.101.3.4.2.2" => "sha384",
        "2.16.840.1.101.3.4.2.3" => "sha512",
        _ => return None,
    })
}

pub fn seq_alg_oid(data: &[u8], t: &Tlv) -> Option<String> {
    if t.tag != 0x30 {
        return None;
    }
    let first = children(data, t).into_iter().next()?;
    if first.tag != 0x06 {
        return None;
    }
    Some(oid(first.bytes(data)?))
}

#[derive(Debug, Clone, Serialize)]
pub struct CertInfo {
    pub subject: String,
    pub issuer: String,
    pub serial: String,
    pub not_before: String,
    pub not_after: String,
    pub signature_algorithm: String,
    pub subject_key_id: Option<String>,
    pub is_ca: Option<bool>,
    pub notes: Vec<String>,
}

pub fn certificate(der: &[u8]) -> Option<CertInfo> {
    let cert = craft_pkcs7::x509::Certificate::parse(der).ok()?;
    let serial = cert.serial.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let subject_key_id = cert.subject_key_id.as_ref().map(|b| b.iter().map(|x| format!("{x:02x}")).collect());
    Some(CertInfo {
        subject: cert.subject.common_name().unwrap_or_default().to_string(),
        issuer: cert.issuer.common_name().unwrap_or_default().to_string(),
        serial,
        not_before: cert.not_before.to_pdf(),
        not_after: cert.not_after.to_pdf(),
        signature_algorithm: String::new(),
        subject_key_id,
        is_ca: Some(cert.is_ca),
        notes: Vec::new(),
    })
}

pub fn spki_parts(spki: &[u8]) -> Option<(String, &[u8])> {
    let tlv = craft_pkcs7::der::Tlv::parse_all_ber(spki).ok()?;
    let children = tlv.children().ok()?;
    let [alg, key] = children.as_slice() else { return None };
    let alg_oid = alg.children().ok()?.into_iter().next()?.oid().ok()?;
    let key_bytes = key.bits().ok()?;
    Some((alg_oid, key_bytes))
}

pub fn certificate_spki(der: &[u8]) -> Option<&[u8]> {
    let cert = craft_pkcs7::x509::Certificate::parse(der).ok()?;
    // Find spki slice in raw cert? Or return None if not directly exposed.
    let _ = cert;
    None
}

pub struct CertParts<'a> {
    pub tbs: &'a [u8],
    pub signature_algorithm: String,
    pub signature_algorithm_der: &'a [u8],
    pub signed_signature_algorithm_der: &'a [u8],
    pub signature: &'a [u8],
    pub subject: &'a [u8],
    pub issuer: &'a [u8],
    pub spki: &'a [u8],
}

pub fn certificate_parts(der: &[u8]) -> Option<CertParts<'_>> {
    let tlv = craft_pkcs7::der::Tlv::parse_all_ber(der).ok()?;
    let top = tlv.children().ok()?;
    let [tbs, alg, sig] = top.as_slice() else { return None };
    let signature_algorithm = alg.children().ok()?.into_iter().next()?.oid().ok()?;
    let signature = sig.bits().ok()?;
    let tbs_children = tbs.children().ok()?;
    let mut idx = 0;
    if tbs_children.first().is_some_and(|t| t.tag == 0xa0) {
        idx = 1;
    }
    let signed_signature_algorithm_der = tbs_children.get(idx + 1)?.raw;
    let issuer = tbs_children.get(idx + 2)?.raw;
    let subject = tbs_children.get(idx + 4)?.raw;
    let spki = tbs_children.get(idx + 5)?.raw;
    Some(CertParts {
        tbs: tbs.raw,
        signature_algorithm,
        signature_algorithm_der: alg.raw,
        signed_signature_algorithm_der,
        signature,
        subject,
        issuer,
        spki,
    })
}
