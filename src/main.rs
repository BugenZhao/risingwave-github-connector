use risingwave_github_connector::GitHubConnectorService;
use risingwave_pb::connector_service::connector_service_server::ConnectorServiceServer;
use tokio::runtime::Runtime;

fn main() {
    dotenv::dotenv().ok();

    Runtime::new()
        .unwrap()
        .block_on(
            tonic::transport::Server::builder()
                .add_service(ConnectorServiceServer::new(GitHubConnectorService))
                .serve("127.0.0.1:50051".parse().unwrap()),
        )
        .unwrap();
}
