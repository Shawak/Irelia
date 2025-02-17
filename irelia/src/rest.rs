//! Module containing all the data for the rest LCU bindings

use std::ops::Deref;

#[cfg(feature = "batched")]
use futures_util::StreamExt;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::{utils::process_info::get_running_client, LCUError, RequestClient};

#[derive(Default)]
/// Struct representing a connection to the LCU
pub struct LCUClient {
    url: String,
    auth_header: String,
}

#[cfg(feature = "batched")]
/// Enum representing the different requests that can be sent to the LCU
pub enum RequestType<'a> {
    Delete,
    Get,
    Head,
    Patch(Option<&'a dyn erased_serde::Serialize>),
    Post(Option<&'a dyn erased_serde::Serialize>),
    Put(Option<&'a dyn erased_serde::Serialize>),
}

#[cfg(feature = "batched")]
/// Struct representing a batched request, taking the
/// request type and endpoint
pub struct BatchRequests<'a> {
    pub request_type: RequestType<'a>,
    pub endpoint: &'a dyn Deref<Target = str>,
}

#[cfg(feature = "batched")]
impl<'a> BatchRequests<'a> {
    /// Creates a new batched request, which can be wrapped in a slice and send to the LCU
    pub fn new(
        request_type: RequestType<'a>,
        endpoint: &'a dyn Deref<Target = str>,
    ) -> BatchRequests<'a> {
        BatchRequests {
            request_type,
            endpoint,
        }
    }
}

impl LCUClient {
    /// Attempts to create a connection to the LCU, errors if it fails
    /// to spin up the child process, or fails to get data from the client.
    /// On Linux, this can happen well the client launches, but on windows
    /// it should be possible to connect well it spins up.
    pub fn new() -> Result<LCUClient, LCUError> {
        let port_pass = get_running_client()?;
        Ok(LCUClient {
            url: port_pass.0,
            auth_header: port_pass.1,
        })
    }

    /// Returns a reference to the URL in use
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns a reference to the auth header in use
    pub fn auth_header(&self) -> &str {
        &self.auth_header
    }

    #[cfg(feature = "batched")]
    /// System for batching requests to the LCU by sending a slice
    /// The buffer size is how many requests can be operated on at
    /// the same time, returns a vector with all the replies
    pub async fn batched<'a, R>(
        &self,
        requests: &[BatchRequests<'a>],
        buffer_size: usize,
        request_client: &RequestClient,
    ) -> Vec<Result<Option<R>, LCUError>>
    where
        R: DeserializeOwned,
    {
        futures_util::stream::iter(requests.iter().map(|request| async move {
            match &request.request_type {
                RequestType::Delete => self.delete(&**request.endpoint, request_client).await,
                RequestType::Get => self.get(&**request.endpoint, request_client).await,
                RequestType::Head => self.head(&**request.endpoint, request_client).await,
                RequestType::Patch(body) => {
                    self.patch(&**request.endpoint, *body, request_client).await
                }
                RequestType::Post(body) => {
                    self.post(&**request.endpoint, *body, request_client).await
                }
                RequestType::Put(body) => {
                    self.put(&**request.endpoint, *body, request_client).await
                }
            }
        }))
        .buffered(buffer_size)
        .collect()
        .await
    }

    /// Sends a delete request to the LCU
    pub async fn delete<'a, R, S>(
        &self,
        endpoint: S,
        request_client: &RequestClient,
    ) -> Result<Option<R>, LCUError>
    where
        R: DeserializeOwned,
        S: Deref<Target = str>,
    {
        self.lcu_request::<(), R, _>(endpoint, "DELETE", None, request_client)
            .await
    }

    /// Sends a get request to the LCU
    pub async fn get<'a, R, S>(
        &self,
        endpoint: S,
        request_client: &RequestClient,
    ) -> Result<Option<R>, LCUError>
    where
        R: DeserializeOwned,
        S: Deref<Target = str>,
    {
        self.lcu_request::<(), R, _>(endpoint, "GET", None, request_client)
            .await
    }

    /// Sends a head request to the LCU
    pub async fn head<'a, R, S>(
        &self,
        endpoint: S,
        request_client: &RequestClient,
    ) -> Result<Option<R>, LCUError>
    where
        R: DeserializeOwned,
        S: Deref<Target = str>,
    {
        self.lcu_request::<(), R, _>(endpoint, "HEAD", None, request_client)
            .await
    }

    /// Sends a patch request to the LCU
    pub async fn patch<'a, T, R, S>(
        &self,
        endpoint: S,
        body: Option<T>,
        request_client: &RequestClient,
    ) -> Result<Option<R>, LCUError>
    where
        T: Serialize,
        R: DeserializeOwned,
        S: Deref<Target = str>,
    {
        self.lcu_request(endpoint, "PATCH", body, request_client)
            .await
    }

    /// Sends a post request to the LCU
    pub async fn post<'a, T, R, S>(
        &self,
        endpoint: S,
        body: Option<T>,
        request_client: &RequestClient,
    ) -> Result<Option<R>, LCUError>
    where
        T: Serialize,
        R: DeserializeOwned,
        S: Deref<Target = str>,
    {
        self.lcu_request(endpoint, "POST", body, request_client)
            .await
    }

    /// Sends a put request to the LCU
    pub async fn put<'a, T, R, S>(
        &self,
        endpoint: S,
        body: Option<T>,
        request_client: &RequestClient,
    ) -> Result<Option<R>, LCUError>
    where
        T: Serialize,
        R: DeserializeOwned,
        S: Deref<Target = str>,
    {
        self.lcu_request(endpoint, "PUT", body, request_client)
            .await
    }

    async fn lcu_request<'a, T, R, S>(
        &self,
        endpoint: S,
        method: &str,
        body: Option<T>,
        request_client: &RequestClient,
    ) -> Result<Option<R>, LCUError>
    where
        T: Serialize,
        R: DeserializeOwned,
        S: Deref<Target = str>,
    {
        request_client
            .request_template(
                &self.url,
                &endpoint,
                method,
                body,
                Some(&self.auth_header),
                |bytes| {
                    if bytes.is_empty() {
                        Ok(None)
                    } else {
                        match serde_json::from_slice(&bytes) {
                            Ok(some) => Ok(Some(some)),
                            Err(err) => Err(LCUError::SerdeJsonError(err)),
                        }
                    }
                },
            )
            .await
    }
}

#[cfg(feature = "batched")]
#[cfg(test)]
mod tests {
    use crate::RequestClient;

    #[tokio::test]
    async fn batch_test() {
        use crate::rest::{BatchRequests, LCUClient, RequestType};

        let page = serde_json::json!(
            {
              "blocks": [
                {
                  "items": [
                    {
                      "count": 1,
                      "id": "3153"
                    },
                  ],
                  "type": "Final Build"
                }
              ],
              "title": "Test Build",
            }
        );
        let client = RequestClient::new();

        let lcu_client = LCUClient::new().unwrap();

        let request: &serde_json::Value = &lcu_client
            .get("/lol-summoner/v2/current-summoner", &client)
            .await
            .unwrap()
            .unwrap();

        let id = &request["summonerId"];

        let endpoint = format!("/lol-item-sets/v1/item-sets/{id}/sets");
        let mut json = lcu_client
            .get::<serde_json::Value, _>(endpoint, &client)
            .await
            .unwrap()
            .unwrap();

        json["itemSets"].as_array_mut().unwrap().push(page);

        let req = BatchRequests {
            request_type: RequestType::Put(Some(&json)),
            endpoint: &format!("/lol-item-sets/v1/item-sets/{id}/sets"),
        };

        let res = lcu_client
            .batched::<serde_json::Value>(&[req], 1, &client)
            .await;

        println!("{:?}", res);

        let a = lcu_client
            .put::<_, serde_json::Value, _>(
                format!("/lol-item-sets/v1/item-sets/{id}/sets"),
                Some(json),
                &client,
            )
            .await;
        println!("{:?}", a);
    }
}
