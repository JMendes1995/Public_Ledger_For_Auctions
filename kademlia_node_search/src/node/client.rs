use std::time::{SystemTime, UNIX_EPOCH};
use tonic::transport::Endpoint;
use tonic::{Request, Status};
use crate::kademlia::kademlia_client::KademliaClient;
use crate::kademlia::{PingRequest, PingResponse, FindNodeRequest, FindNodeResponse};
use crate::node::crypto::Crypto;
use ring::signature::{Ed25519KeyPair, KeyPair};
use tokio::time::{timeout, Duration};
use crate::config::{TIMEOUT_MAX_ATTEMPTS, TIMEOUT_TIMER};
use colored::Colorize;

pub struct Client;

impl Client {
    pub fn create_ping_request(keypair: &Ed25519KeyPair, addr: String) -> PingRequest {
        let node_address = addr;
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let message = format!("{}{}", node_address, timestamp).into_bytes();
        let signature = Crypto::sign_message(keypair, &message);
        let sender_public_key = keypair.public_key().as_ref().to_vec();

        PingRequest {
            node_address,
            timestamp,
            signature,
            sender_public_key,
        }
    }

    async fn attempt_with_timeout<F, Fut, T>(&self, mut attempt: F) -> Result<T, Status>
    where
        F: FnMut() -> Fut + Send,
        Fut: std::future::Future<Output = Result<T, Status>> + Send,
        T: Send,
    {
        for _ in 0..TIMEOUT_MAX_ATTEMPTS {
            let result = timeout(Duration::from_secs(TIMEOUT_TIMER), attempt()).await;
            match result {
                Ok(Ok(response)) => return Ok(response),
                Ok(Err(e)) => eprintln!("{}", format!("Attempt failed: {}", e).red()),
                Err(_) => eprintln!("{}", "Attempt timed out".red()),
            }
        }
        Err(Status::internal("All attempts to send the request failed"))
    }

    pub async fn send_ping_request(&self, keypair: &Ed25519KeyPair, addr: String, server_addr: String) -> Result<PingResponse, Status> {
        let endpoint = Endpoint::from_shared(format!("http://{}", server_addr))
            .map_err(|e| Status::internal(format!("Failed to create endpoint: {}", e)))?;

        self.attempt_with_timeout(|| {
            let endpoint = endpoint.clone();
            let ping_request = Self::create_ping_request(keypair, addr.clone());
            async move {
                let channel = endpoint.connect().await
                    .map_err(|e| Status::internal(format!("Failed to connect: {}", e)))?;
                let mut client = KademliaClient::new(channel);
                let request = Request::new(ping_request);
                client.ping(request).await.map(|response| response.into_inner())
            }
        }).await
    }

    pub async fn send_find_node_request(&self, target_node_id: Vec<u8>, target_address: String, requester_node_id: Vec<u8>, requester_node_address: String) -> Result<FindNodeResponse, Status> {
        let endpoint = Endpoint::from_shared(format!("http://{}", target_address))
            .map_err(|e| Status::internal(format!("Failed to create endpoint: {}", e)))?;

        self.attempt_with_timeout(|| {
            let endpoint = endpoint.clone();
            let target_node_id = target_node_id.clone();
            let requester_node_id = requester_node_id.clone();
            let requester_node_address = requester_node_address.clone();
            async move {
                let channel = endpoint.connect().await
                    .map_err(|e| Status::internal(format!("Failed to connect: {}", e)))?;
                let mut client = KademliaClient::new(channel);
                let find_node_request = FindNodeRequest {
                    requester_node_id,
                    requester_node_address,
                    target_node_id,
                };
                let request = Request::new(find_node_request);
                client.find_node(request).await.map(|response| response.into_inner())
            }
        }).await
    }
}
