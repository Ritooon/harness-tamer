//! Client HTTP réservé à la boucle locale (`127.0.0.1` / `::1`).
//!
//! Il sert à dialoguer avec `llama-server` (ou un Ollama local). Il refuse
//! toute adresse qui n'est pas une IP de bouclage littérale — même `localhost`,
//! dont la résolution pourrait être détournée —, n'utilise jamais de proxy et
//! ne suit aucune redirection.

use std::net::{Ipv4Addr, TcpListener};
use std::time::Duration;

use futures_util::StreamExt;
use futures_util::stream::BoxStream;
use serde::Serialize;
use serde::de::DeserializeOwned;
use url::{Host, Url};

use crate::error::NetError;

/// Vérifie qu'une URL vise une IP de bouclage en HTTP.
pub fn ensure_loopback(url: &Url) -> Result<(), NetError> {
    let loopback = match url.host() {
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    if url.scheme() == "http" && loopback {
        Ok(())
    } else {
        Err(NetError::NotLoopback(url.to_string()))
    }
}

/// Réserve un port libre sur 127.0.0.1 pour y lancer `llama-server`.
pub fn free_loopback_port() -> Result<u16, NetError> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    Ok(listener.local_addr()?.port())
}

#[derive(Clone)]
pub struct LoopbackClient {
    base: Url,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl std::fmt::Debug for LoopbackClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // La clé d'API ne doit jamais apparaître dans les journaux de débogage.
        f.debug_struct("LoopbackClient").field("base", &self.base.as_str()).finish()
    }
}

impl LoopbackClient {
    pub fn new(base: Url, api_key: Option<String>) -> Result<Self, NetError> {
        ensure_loopback(&base)?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .build()?;
        Ok(Self { base, api_key, client })
    }

    pub fn base(&self) -> &Url {
        &self.base
    }

    /// Construit l'URL d'un chemin et revérifie qu'elle reste locale (un chemin
    /// absolu comme `http://ailleurs/` remplacerait sinon l'hôte).
    fn url(&self, path: &str) -> Result<Url, NetError> {
        let url = self.base.join(path).map_err(|e| NetError::InvalidUrl(e.to_string()))?;
        ensure_loopback(&url)?;
        if url.port_or_known_default() != self.base.port_or_known_default() {
            return Err(NetError::NotLoopback(url.to_string()));
        }
        Ok(url)
    }

    fn request(&self, method: reqwest::Method, path: &str) -> Result<reqwest::RequestBuilder, NetError> {
        let mut req = self.client.request(method, self.url(path)?);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        Ok(req)
    }

    /// Statut HTTP d'un GET (sonde de santé `/health`).
    pub async fn get_status(&self, path: &str) -> Result<u16, NetError> {
        Ok(self.request(reqwest::Method::GET, path)?.send().await?.status().as_u16())
    }

    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, NetError> {
        let resp = self.request(reqwest::Method::GET, path)?.send().await?.error_for_status()?;
        resp.json().await.map_err(|e| NetError::Json(e.to_string()))
    }

    pub async fn post_json<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, NetError> {
        let resp = self.request(reqwest::Method::POST, path)?.json(body).send().await?.error_for_status()?;
        resp.json().await.map_err(|e| NetError::Json(e.to_string()))
    }

    /// POST dont la réponse est lue en flux (Server-Sent Events du chat).
    pub async fn post_stream<B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<BoxStream<'static, Result<Vec<u8>, NetError>>, NetError> {
        let resp = self.request(reqwest::Method::POST, path)?.json(body).send().await?.error_for_status()?;
        Ok(resp.bytes_stream().map(|chunk| chunk.map(|b| b.to_vec()).map_err(NetError::from)).boxed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_literal_loopback_ips_are_accepted() {
        assert!(ensure_loopback(&Url::parse("http://127.0.0.1:8080/").unwrap()).is_ok());
        assert!(ensure_loopback(&Url::parse("http://[::1]:8080/").unwrap()).is_ok());
        assert!(ensure_loopback(&Url::parse("http://localhost:8080/").unwrap()).is_err());
        assert!(ensure_loopback(&Url::parse("http://192.168.1.10:8080/").unwrap()).is_err());
        assert!(ensure_loopback(&Url::parse("https://127.0.0.1:8080/").unwrap()).is_err());
        assert!(ensure_loopback(&Url::parse("http://example.org/").unwrap()).is_err());
    }

    #[test]
    fn a_path_cannot_escape_the_loopback_base() {
        let client = LoopbackClient::new(Url::parse("http://127.0.0.1:9000/").unwrap(), None).unwrap();
        assert!(client.url("/v1/chat/completions").is_ok());
        assert!(client.url("http://example.org/steal").is_err());
        assert!(client.url("http://127.0.0.1:9001/other-port").is_err());
    }

    #[test]
    fn debug_output_hides_the_api_key() {
        let client =
            LoopbackClient::new(Url::parse("http://127.0.0.1:9000/").unwrap(), Some("secret".into())).unwrap();
        assert!(!format!("{client:?}").contains("secret"));
    }

    #[test]
    fn free_port_is_non_zero() {
        assert_ne!(free_loopback_port().unwrap(), 0);
    }
}
