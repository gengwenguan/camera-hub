use anyhow::Result;

/// APK workers are standalone processes, so they cannot use the JNI-backed
/// Android platform verifier. Other deployments keep their platform roots.
pub fn builder() -> Result<reqwest::ClientBuilder> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let builder = reqwest::Client::builder();
    #[cfg(feature = "android-app")]
    {
        let roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
            .iter()
            .map(|certificate| reqwest::Certificate::from_der(certificate.as_ref()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        return Ok(builder.tls_certs_only(roots));
    }
    #[cfg(not(feature = "android-app"))]
    {
        Ok(builder)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn client_builder_is_usable() {
        super::builder().unwrap().build().unwrap();
    }
}
