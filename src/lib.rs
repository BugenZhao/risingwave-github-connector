#[cfg(test)]
mod tests;

use std::time::Duration;

use futures::{
    pin_mut,
    stream::{empty, BoxStream},
    StreamExt, TryStreamExt,
};
use octocrab::{etag::EntityTag, models::events::Event};
use risingwave_pb::connector_service::{connector_service_server::ConnectorService, *};
use serde_json::json;
use tokio::sync::mpsc::unbounded_channel;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::{async_trait, Request, Result, Status};

pub struct GitHubConnectorService;

#[async_trait]
impl ConnectorService for GitHubConnectorService {
    type GetEventStreamStream = BoxStream<'static, Result<GetEventStreamResponse>>;
    type SinkStreamStream = BoxStream<'static, Result<SinkResponse>>;

    async fn sink_stream(
        &self,
        _request: Request<tonic::Streaming<SinkStreamRequest>>,
    ) -> Result<tonic::Response<Self::SinkStreamStream>> {
        Err(Status::unimplemented("not implemented"))
    }

    async fn get_event_stream(
        &self,
        request: Request<GetEventStreamRequest>,
    ) -> Result<tonic::Response<Self::GetEventStreamStream>> {
        let request = request.into_inner().request.unwrap();

        let stream = match request {
            get_event_stream_request::Request::Validate(_) => empty().boxed(),
            get_event_stream_request::Request::Start(start_msg) => {
                spawn(start_msg.source_id, start_msg.start_offset).boxed()
            }
        };

        Ok(tonic::Response::new(stream))
    }

    async fn validate_sink(
        &self,
        _request: Request<ValidateSinkRequest>,
    ) -> Result<tonic::Response<ValidateSinkResponse>> {
        Err(Status::unimplemented("not implemented"))
    }
}

struct FakeCdcMessage {
    event: Event,
}

impl FakeCdcMessage {
    fn to_json(&self) -> String {
        json!({
            "payload": {
                "before": null,
                "after": {
                    "id": self.event.id,
                    "name": self.event.actor.login,
                    "event_type": self.event.r#type,
                    "created_at": self.event.created_at,
                },
                "op": "c",
            }
        })
        .to_string()
    }
}

fn spawn(source_id: u64, mut offset: String) -> BoxStream<'static, Result<GetEventStreamResponse>> {
    let (tx, rx) = unbounded_channel();
    let tx2 = tx.clone();

    let crab = octocrab::OctocrabBuilder::new()
        .personal_token(std::env::var("GITHUB_TOKEN").unwrap())
        .base_url("https://api.github.com/repos/risingwavelabs/risingwave/")
        .unwrap()
        .build()
        .unwrap();

    let task = async move {
        loop {
            let etag: Option<EntityTag> = offset.parse().ok();
            let events = crab.events().etag(etag).send().await?;
            let new_offset = events.etag.map(|etag| etag.to_string()).unwrap_or_default();

            if let Some(events) = events.value {
                let stream = events.into_stream(&crab);
                pin_mut!(stream);

                let mut messages = Vec::new();

                while let Some(event) = stream.try_next().await? {
                    let message = CdcMessage {
                        payload: FakeCdcMessage { event }.to_json(),
                        partition: "0".to_string(),
                        offset: offset.clone(),
                    };
                    messages.push(message);
                }

                if let Some(message) = messages.last_mut() {
                    message.offset = new_offset.clone();
                }

                if tx
                    .send(Ok(GetEventStreamResponse {
                        source_id,
                        events: messages,
                    }))
                    .is_err()
                {
                    break;
                }

                offset = new_offset;
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        }

        Ok::<_, anyhow::Error>(())
    };

    tokio::spawn(async move {
        if let Err(err) = task.await {
            println!("error: {}", err);
            tx2.send(Err(Status::internal(err.to_string()))).unwrap();
        }
    });

    println!("spawned stream for source {}", source_id);

    UnboundedReceiverStream::new(rx).boxed()
}
