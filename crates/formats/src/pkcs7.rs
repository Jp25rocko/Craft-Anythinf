use crate::verify::{Digest, PssParameters, SigScheme};

#[derive(Debug, Clone)]
pub struct SignedData {
    pub certificates: Vec<Vec<u8>>,
    pub digest: Option<Digest>,
    pub signature_algorithm: String,
    pub pss_parameters: Option<Result<PssParameters, &'static str>>,
    pub signature: Vec<u8>,
    pub signed_attrs_reencoded: Option<Vec<u8>>,
    pub signed_attrs_error: Option<&'static str>,
    pub message_digest: Option<String>,
    pub content_type: Option<String>,
    pub signer_issuer: Vec<u8>,
    pub signer_serial: Vec<u8>,
    pub signer_key_identifier: Option<Vec<u8>>,
    pub signer_identity_notes: Vec<String>,
    pub non_minimal_length: bool,
    pub econtent: Option<Vec<u8>>,
    signer_cert_der: Option<Vec<u8>>,
}

impl SignedData {
    pub fn signer_cert(&self) -> Option<&[u8]> {
        self.signer_cert_der.as_deref()
    }

    pub fn scheme(&self) -> Option<SigScheme> {
        let d = self.digest?;
        match self.signature_algorithm.as_str() {
            "1.2.840.113549.1.1.1"
            | "1.2.840.113549.1.1.5"
            | "1.2.840.113549.1.1.11"
            | "1.2.840.113549.1.1.12"
            | "1.2.840.113549.1.1.13" => Some(SigScheme::RsaPkcs1v15(d)),
            "1.2.840.113549.1.1.10" => Some(SigScheme::RsaPss(d)),
            "1.2.840.10045.2.1"
            | "1.2.840.10045.4.3.2"
            | "1.2.840.10045.4.3.3"
            | "1.2.840.10045.4.3.4" => Some(SigScheme::Ecdsa(d)),
            _ => None,
        }
    }
}

pub(crate) fn digest_from_oid(oid: &str) -> Option<Digest> {
    Some(match oid {
        "1.3.14.3.2.26" => Digest::Sha1,
        "2.16.840.1.101.3.4.2.1" => Digest::Sha256,
        "2.16.840.1.101.3.4.2.2" => Digest::Sha384,
        "2.16.840.1.101.3.4.2.3" => Digest::Sha512,
        _ => return None,
    })
}

pub fn parse_signed_data(data: &[u8]) -> Option<SignedData> {
    let sd = craft_pkcs7::cms::SignedData::parse(data).ok()?;
    let certificates: Vec<Vec<u8>> = sd.certificates.iter().map(|c| c.raw.clone()).collect();
    let digest = match sd.signer.digest {
        craft_pkcs7::DigestAlg::Sha1 => Some(Digest::Sha1),
        craft_pkcs7::DigestAlg::Sha256 => Some(Digest::Sha256),
        craft_pkcs7::DigestAlg::Sha384 => Some(Digest::Sha384),
        craft_pkcs7::DigestAlg::Sha512 => Some(Digest::Sha512),
        _ => None,
    };
    let signer_cert_der = sd.signer_certificate().map(|c| c.raw.clone());
    let message_digest = sd.signer.message_digest.as_ref().map(|b| b.iter().map(|x| format!("{x:02x}")).collect());
    let non_minimal_length = !sd.quirks.is_empty();
    Some(SignedData {
        certificates,
        digest,
        signature_algorithm: String::new(),
        pss_parameters: None,
        signature: sd.signer.signature.clone(),
        signed_attrs_reencoded: sd.signer.signed_attrs.clone(),
        signed_attrs_error: None,
        message_digest,
        content_type: Some(sd.econtent_type.clone()),
        signer_issuer: Vec::new(),
        signer_serial: Vec::new(),
        signer_key_identifier: None,
        signer_identity_notes: Vec::new(),
        non_minimal_length,
        econtent: sd.content.clone(),
        signer_cert_der,
    })
}
