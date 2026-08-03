#![deny(clippy::all, clippy::cargo)]
#![warn(clippy::nursery, clippy::pedantic)]
#![allow(clippy::cargo_common_metadata)]
// 依存クレート同士が同一クレートの別バージョンに依存しているため、このクレートからは解消できない
#![allow(clippy::multiple_crate_versions)]

mod domain {
    use anyhow::anyhow;
    use indexmap::IndexMap;
    use prost::bytes::Buf;
    use std::fmt::Debug;
    use std::str::Utf8Error;

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct PlayerUuidString([u8; 36]);

    impl PlayerUuidString {
        pub const fn as_str(&self) -> Result<&str, Utf8Error> {
            std::str::from_utf8(&self.0)
        }

        pub fn from_string(str: &String) -> anyhow::Result<Self> {
            if !str.is_ascii() {
                Err(anyhow!("Expected ascii string for UuidString, got {str}"))
            } else if str.len() != 36 {
                Err(anyhow!(
                    "Expect string of length 36 for UuidString, got {str}"
                ))
            } else {
                let mut result: [u8; 36] = [0; 36];
                str.as_bytes().copy_to_slice(result.as_mut_slice());
                Ok(Self(result))
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct Player {
        pub uuid: PlayerUuidString,
    }

    #[derive(Debug, Clone)]
    pub struct PlayerBreakCount {
        pub player: Player,
        pub break_count: u64,
    }

    #[derive(Debug, Clone)]
    pub struct PlayerBuildCount {
        pub player: Player,
        pub build_count: u64,
    }

    #[derive(Debug, Clone)]
    pub struct PlayerPlayTicks {
        pub player: Player,
        pub play_ticks: u64,
    }

    #[derive(Debug, Clone)]
    pub struct PlayerVoteCount {
        pub player: Player,
        pub vote_count: u64,
    }

    #[derive(Debug, Clone, Default)]
    pub struct AggregatedPlayerData {
        pub break_count: u64,
        pub build_count: u64,
        pub play_ticks: u64,
        pub vote_count: u64,
    }

    #[derive(Debug, Clone, Default)]
    pub struct KnownAggregatedPlayerData(pub IndexMap<Player, AggregatedPlayerData>);

    #[async_trait::async_trait]
    pub trait PlayerDataRepository: Debug + Sync + Send + 'static {
        async fn get_all_break_counts(&self) -> anyhow::Result<Vec<PlayerBreakCount>>;
        async fn get_all_build_counts(&self) -> anyhow::Result<Vec<PlayerBuildCount>>;
        async fn get_all_play_ticks(&self) -> anyhow::Result<Vec<PlayerPlayTicks>>;
        async fn get_all_vote_counts(&self) -> anyhow::Result<Vec<PlayerVoteCount>>;
    }
}

mod use_cases {
    use crate::domain::{AggregatedPlayerData, KnownAggregatedPlayerData, PlayerDataRepository};
    use indexmap::IndexMap;
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    pub struct GetAllPlayerDataUseCase {
        pub repository: Arc<dyn PlayerDataRepository>,
    }

    impl GetAllPlayerDataUseCase {
        #[tracing::instrument]
        pub async fn get_all_known_aggregated_player_data(
            &self,
        ) -> anyhow::Result<KnownAggregatedPlayerData> {
            let (break_counts, build_counts, play_ticks, vote_counts) = tokio::try_join!(
                self.repository.get_all_break_counts(),
                self.repository.get_all_build_counts(),
                self.repository.get_all_play_ticks(),
                self.repository.get_all_vote_counts(),
            )?;

            let mut result_map: IndexMap<_, AggregatedPlayerData> =
                IndexMap::with_capacity(break_counts.len());

            for break_count in break_counts {
                let entry = result_map.entry(break_count.player).or_default();
                entry.break_count = break_count.break_count;
            }

            for build_count in build_counts {
                let entry = result_map.entry(build_count.player).or_default();
                entry.build_count = build_count.build_count;
            }

            for tick_count in play_ticks {
                let entry = result_map.entry(tick_count.player).or_default();
                entry.play_ticks = tick_count.play_ticks;
            }

            for vote_count in vote_counts {
                let entry = result_map.entry(vote_count.player).or_default();
                entry.vote_count = vote_count.vote_count;
            }

            Ok(KnownAggregatedPlayerData(result_map))
        }
    }
}

mod infra_axum_handlers {
    use crate::domain::PlayerDataRepository;
    use crate::use_cases::GetAllPlayerDataUseCase;
    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use std::sync::Arc;

    #[derive(Clone, Debug)]
    pub struct SharedAppState {
        pub repository: Arc<dyn PlayerDataRepository>,
    }

    mod presenter {
        use crate::domain::{KnownAggregatedPlayerData, Player};
        use std::fmt::Write;

        fn estimate_presented_string_size(data: &KnownAggregatedPlayerData) -> usize {
            // Each Prometheus record takes about 85 characters and 4 records are generated per
            // aggregated player data, hence length * 340. The constant term is from the help string.
            100 + data.0.len() * 340
        }

        fn write_record(
            target: &mut String,
            player: &Player,
            kind: &'static str,
            value: u64,
        ) -> anyhow::Result<()> {
            Ok(target.write_str(&format!(
                r#"player_data{{uuid="{}",kind="{}"}} {}{}"#,
                player.uuid.as_str()?,
                kind,
                value,
                '\n'
            ))?)
        }

        #[tracing::instrument]
        pub fn present_player_data_as_prometheus_metrics(
            data: &KnownAggregatedPlayerData,
        ) -> anyhow::Result<String> {
            let mut result = String::with_capacity(estimate_presented_string_size(data));

            result
                .write_str("# HELP player_data Player metrics, partitioned by uuid and kind\n")?;
            result.write_str("# TYPE player_data gauge\n")?;

            for (player, data) in &data.0 {
                write_record(&mut result, player, "break_count", data.break_count)?;
                write_record(&mut result, player, "build_count", data.build_count)?;
                write_record(&mut result, player, "play_ticks", data.play_ticks)?;
                write_record(&mut result, player, "vote_count", data.vote_count)?;
            }

            Ok(result)
        }
    }

    const fn const_error_response() -> (StatusCode, &'static str) {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Encountered internal server error. Please contact the server administrator to resolve the issue.",
        )
    }

    pub async fn handle_get_metrics(State(state): State<SharedAppState>) -> Response {
        // we need a separate handler function to create an error tracing span
        #[tracing::instrument]
        async fn handler(state: &SharedAppState) -> Response {
            let use_case = GetAllPlayerDataUseCase {
                repository: state.repository.clone(),
            };

            match use_case
                .get_all_known_aggregated_player_data()
                .await
                .and_then(|known_aggregated_player_data| {
                    presenter::present_player_data_as_prometheus_metrics(
                        &known_aggregated_player_data,
                    )
                }) {
                Ok(metrics_presentation) => (StatusCode::OK, metrics_presentation).into_response(),
                Err(e) => {
                    tracing::error!("{:?}", e);
                    const_error_response().into_response()
                }
            }
        }

        handler(&state).await
    }
}

mod infra_repository_impls {
    #[allow(dead_code)]
    #[allow(
        clippy::nursery,
        clippy::pedantic,
        clippy::derive_partial_eq_without_eq
    )]
    pub(crate) mod buf_generated {
        include!("gen/mod.rs");
    }

    pub mod config {
        #[derive(serde::Deserialize, Debug, Clone)]
        pub struct GrpcClientConfig {
            pub game_data_server_grpc_endpoint_url: String,
        }

        impl GrpcClientConfig {
            pub fn from_env() -> anyhow::Result<Self> {
                Ok(envy::from_env::<Self>()?)
            }
        }
    }

    mod buf_generated_to_domain {
        use super::buf_generated::gigantic_minecraft::seichi_game_data::v1 as generated;
        use crate::domain;
        use crate::domain::PlayerUuidString;

        fn into_domain_player(p: &generated::Player) -> anyhow::Result<domain::Player> {
            Ok(domain::Player {
                uuid: PlayerUuidString::from_string(&p.uuid)?,
            })
        }

        #[tracing::instrument]
        fn extract_domain_player(
            player: Option<generated::Player>,
        ) -> anyhow::Result<domain::Player> {
            let player = player.ok_or_else(|| anyhow::anyhow!("Player field not set"))?;

            into_domain_player(&player)
        }

        #[tracing::instrument]
        pub fn try_into_domain_player_break_count(
            value: generated::PlayerBreakCount,
        ) -> anyhow::Result<domain::PlayerBreakCount> {
            Ok(domain::PlayerBreakCount {
                player: extract_domain_player(value.player)?,
                break_count: value.break_count,
            })
        }

        #[tracing::instrument]
        pub fn try_into_domain_player_build_count(
            value: generated::PlayerBuildCount,
        ) -> anyhow::Result<domain::PlayerBuildCount> {
            Ok(domain::PlayerBuildCount {
                player: extract_domain_player(value.player)?,
                build_count: value.build_count,
            })
        }

        #[tracing::instrument]
        pub fn try_into_domain_player_play_ticks(
            value: generated::PlayerPlayTicks,
        ) -> anyhow::Result<domain::PlayerPlayTicks> {
            Ok(domain::PlayerPlayTicks {
                player: extract_domain_player(value.player)?,
                play_ticks: value.play_ticks,
            })
        }

        #[tracing::instrument]
        pub fn try_into_domain_player_vote_count(
            value: generated::PlayerVoteCount,
        ) -> anyhow::Result<domain::PlayerVoteCount> {
            Ok(domain::PlayerVoteCount {
                player: extract_domain_player(value.player)?,
                vote_count: value.vote_count,
            })
        }
    }

    use buf_generated::gigantic_minecraft::seichi_game_data::v1::read_service_client::ReadServiceClient;
    type GameDataGrpcClient = ReadServiceClient<tonic::transport::Channel>;

    #[derive(Debug)]
    pub struct GameDataGrpcRepository {
        client: GameDataGrpcClient,
    }

    impl GameDataGrpcRepository {
        #[tracing::instrument]
        pub async fn initialize_connections_with(
            config: config::GrpcClientConfig,
        ) -> anyhow::Result<Self> {
            let client =
                GameDataGrpcClient::connect(config.game_data_server_grpc_endpoint_url).await?;

            Ok(Self { client })
        }

        pub(crate) fn game_data_client(&self) -> GameDataGrpcClient {
            self.client.clone()
        }

        #[cfg(test)]
        pub(crate) const fn from_client(client: GameDataGrpcClient) -> Self {
            Self { client }
        }
    }

    fn empty_request() -> tonic::Request<pbjson_types::Empty> {
        tonic::Request::new(pbjson_types::Empty::default())
    }

    use crate::domain::{PlayerBreakCount, PlayerBuildCount, PlayerPlayTicks, PlayerVoteCount};

    #[async_trait::async_trait]
    impl crate::domain::PlayerDataRepository for GameDataGrpcRepository {
        #[tracing::instrument]
        async fn get_all_break_counts(&self) -> anyhow::Result<Vec<PlayerBreakCount>> {
            Ok(self
                .game_data_client()
                .break_counts(empty_request())
                .await?
                .into_inner()
                .results
                .into_iter()
                .map(buf_generated_to_domain::try_into_domain_player_break_count)
                .collect::<Result<_, _>>()?)
        }

        #[tracing::instrument]
        async fn get_all_build_counts(&self) -> anyhow::Result<Vec<PlayerBuildCount>> {
            Ok(self
                .game_data_client()
                .build_counts(empty_request())
                .await?
                .into_inner()
                .results
                .into_iter()
                .map(buf_generated_to_domain::try_into_domain_player_build_count)
                .collect::<Result<_, _>>()?)
        }

        #[tracing::instrument]
        async fn get_all_play_ticks(&self) -> anyhow::Result<Vec<PlayerPlayTicks>> {
            Ok(self
                .game_data_client()
                .play_ticks(empty_request())
                .await?
                .into_inner()
                .results
                .into_iter()
                .map(buf_generated_to_domain::try_into_domain_player_play_ticks)
                .collect::<Result<_, _>>()?)
        }

        #[tracing::instrument]
        async fn get_all_vote_counts(&self) -> anyhow::Result<Vec<PlayerVoteCount>> {
            Ok(self
                .game_data_client()
                .vote_counts(empty_request())
                .await?
                .into_inner()
                .results
                .into_iter()
                .map(buf_generated_to_domain::try_into_domain_player_vote_count)
                .collect::<Result<_, _>>()?)
        }
    }
}

mod app {
    use crate::infra_axum_handlers;
    use crate::infra_axum_handlers::SharedAppState;
    use crate::infra_repository_impls;
    use std::sync::Arc;
    use tower_http::trace::TraceLayer;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    pub fn build_router(shared_state: SharedAppState) -> axum::Router {
        use infra_axum_handlers::handle_get_metrics;

        use axum::routing::get;
        use axum::Router;

        Router::new()
            .route("/metrics", get(handle_get_metrics))
            .with_state(shared_state)
            .layer(TraceLayer::new_for_http())
    }

    pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
        // initialize tracing
        // see https://github.com/tokio-rs/axum/blob/79a0a54bc9f0f585c974b5e6793541baff980662/examples/tracing-aka-logging/src/main.rs
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new(
                std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
            ))
            .with(tracing_subscriber::fmt::layer())
            .init();

        let shared_state = {
            let repository = {
                let client_config = infra_repository_impls::config::GrpcClientConfig::from_env()?;
                let repository =
                    infra_repository_impls::GameDataGrpcRepository::initialize_connections_with(
                        client_config,
                    )
                    .await?;

                Arc::new(repository)
            };

            SharedAppState { repository }
        };

        let app = build_router(shared_state);

        let addr = {
            use std::net::SocketAddr;
            SocketAddr::from(([0, 0, 0, 0], 80))
        };

        let listener = tokio::net::TcpListener::bind(addr).await?;

        tracing::info!("listening on {}", addr);

        Ok(axum::serve(listener, app).await?)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    app::main().await
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        Player, PlayerBreakCount, PlayerBuildCount, PlayerDataRepository, PlayerPlayTicks,
        PlayerUuidString, PlayerVoteCount,
    };

    const TEST_UUID: &str = "00000000-0000-4000-8000-000000000001";

    fn test_player() -> Player {
        Player {
            uuid: PlayerUuidString::from_string(&TEST_UUID.to_string()).unwrap(),
        }
    }

    #[derive(Debug)]
    struct FakeRepository;

    #[async_trait::async_trait]
    impl PlayerDataRepository for FakeRepository {
        async fn get_all_break_counts(&self) -> anyhow::Result<Vec<PlayerBreakCount>> {
            Ok(vec![PlayerBreakCount {
                player: test_player(),
                break_count: 10,
            }])
        }

        async fn get_all_build_counts(&self) -> anyhow::Result<Vec<PlayerBuildCount>> {
            Ok(vec![PlayerBuildCount {
                player: test_player(),
                build_count: 20,
            }])
        }

        async fn get_all_play_ticks(&self) -> anyhow::Result<Vec<PlayerPlayTicks>> {
            Ok(vec![PlayerPlayTicks {
                player: test_player(),
                play_ticks: 30,
            }])
        }

        async fn get_all_vote_counts(&self) -> anyhow::Result<Vec<PlayerVoteCount>> {
            Ok(vec![PlayerVoteCount {
                player: test_player(),
                vote_count: 40,
            }])
        }
    }

    #[derive(Debug)]
    struct FailingRepository;

    #[async_trait::async_trait]
    impl PlayerDataRepository for FailingRepository {
        async fn get_all_break_counts(&self) -> anyhow::Result<Vec<PlayerBreakCount>> {
            Err(anyhow::anyhow!("repository is broken"))
        }

        async fn get_all_build_counts(&self) -> anyhow::Result<Vec<PlayerBuildCount>> {
            Err(anyhow::anyhow!("repository is broken"))
        }

        async fn get_all_play_ticks(&self) -> anyhow::Result<Vec<PlayerPlayTicks>> {
            Err(anyhow::anyhow!("repository is broken"))
        }

        async fn get_all_vote_counts(&self) -> anyhow::Result<Vec<PlayerVoteCount>> {
            Err(anyhow::anyhow!("repository is broken"))
        }
    }

    mod metrics_endpoint {
        use super::{FailingRepository, FakeRepository, TEST_UUID};
        use crate::app;
        use crate::domain::PlayerDataRepository;
        use crate::infra_axum_handlers::SharedAppState;
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use http_body_util::BodyExt;
        use std::sync::Arc;
        use tower::ServiceExt;

        async fn get_metrics_response(
            repository: Arc<dyn PlayerDataRepository>,
        ) -> axum::response::Response {
            app::build_router(SharedAppState { repository })
                .oneshot(
                    Request::builder()
                        .uri("/metrics")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
        }

        #[tokio::test]
        async fn returns_200_with_prometheus_metrics_when_repository_succeeds() {
            let response = get_metrics_response(Arc::new(FakeRepository)).await;
            assert_eq!(response.status(), StatusCode::OK);

            let body = response.into_body().collect().await.unwrap().to_bytes();
            let body = std::str::from_utf8(&body).unwrap();

            assert!(body.contains("# TYPE player_data gauge"));
            for (kind, value) in [
                ("break_count", 10),
                ("build_count", 20),
                ("play_ticks", 30),
                ("vote_count", 40),
            ] {
                let expected =
                    format!(r#"player_data{{uuid="{TEST_UUID}",kind="{kind}"}} {value}"#);
                assert!(
                    body.contains(&expected),
                    "expected `{expected}` in:\n{body}"
                );
            }
        }

        #[tokio::test]
        async fn returns_500_when_repository_fails() {
            let response = get_metrics_response(Arc::new(FailingRepository)).await;
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    mod grpc_contract {
        use crate::domain::PlayerDataRepository;
        use crate::infra_repository_impls::buf_generated::gigantic_minecraft::seichi_game_data::v1 as generated;
        use crate::infra_repository_impls::GameDataGrpcRepository;
        use generated::read_service_client::ReadServiceClient;
        use generated::read_service_server::{ReadService, ReadServiceServer};
        use hyper_util::rt::TokioIo;
        use tonic::transport::{Endpoint, Server, Uri};
        use tonic::{Request, Response, Status};

        fn generated_player() -> generated::Player {
            generated::Player {
                uuid: super::TEST_UUID.to_string(),
                last_known_name: "steve".to_string(),
            }
        }

        #[derive(Debug, Default)]
        struct MockReadService;

        #[tonic::async_trait]
        impl ReadService for MockReadService {
            async fn last_quits(
                &self,
                _: Request<pbjson_types::Empty>,
            ) -> Result<Response<generated::LastQuitsResponse>, Status> {
                Ok(Response::new(generated::LastQuitsResponse {
                    results: vec![],
                }))
            }

            async fn break_counts(
                &self,
                _: Request<pbjson_types::Empty>,
            ) -> Result<Response<generated::BreakCountsResponse>, Status> {
                Ok(Response::new(generated::BreakCountsResponse {
                    results: vec![generated::PlayerBreakCount {
                        player: Some(generated_player()),
                        break_count: 42,
                    }],
                }))
            }

            async fn build_counts(
                &self,
                _: Request<pbjson_types::Empty>,
            ) -> Result<Response<generated::BuildCountsResponse>, Status> {
                Ok(Response::new(generated::BuildCountsResponse {
                    results: vec![generated::PlayerBuildCount {
                        player: Some(generated_player()),
                        build_count: 43,
                    }],
                }))
            }

            async fn play_ticks(
                &self,
                _: Request<pbjson_types::Empty>,
            ) -> Result<Response<generated::PlayTicksResponse>, Status> {
                Ok(Response::new(generated::PlayTicksResponse {
                    results: vec![generated::PlayerPlayTicks {
                        player: Some(generated_player()),
                        play_ticks: 44,
                    }],
                }))
            }

            async fn vote_counts(
                &self,
                _: Request<pbjson_types::Empty>,
            ) -> Result<Response<generated::VoteCountsResponse>, Status> {
                Ok(Response::new(generated::VoteCountsResponse {
                    results: vec![generated::PlayerVoteCount {
                        player: Some(generated_player()),
                        vote_count: 45,
                    }],
                }))
            }
        }

        /// tonic 公式の mock パターン (`tokio::io::duplex` + `connect_with_connector`) で
        /// インプロセスの gRPC サーバーに接続したリポジトリを作る。
        /// <https://github.com/hyperium/tonic/blob/master/examples/src/mock/mock.rs>
        async fn connected_repository() -> GameDataGrpcRepository {
            let (client_io, server_io) = tokio::io::duplex(64 * 1024);

            tokio::spawn(async move {
                Server::builder()
                    .add_service(ReadServiceServer::new(MockReadService))
                    .serve_with_incoming(tokio_stream::once(Ok::<_, std::io::Error>(server_io)))
                    .await
            });

            let mut client_io = Some(client_io);
            let channel = Endpoint::try_from("http://mock.invalid")
                .unwrap()
                .connect_with_connector(tower::service_fn(move |_: Uri| {
                    let io = client_io.take();
                    async move {
                        io.map(TokioIo::new)
                            .ok_or_else(|| std::io::Error::other("duplex client already consumed"))
                    }
                }))
                .await
                .unwrap();

            GameDataGrpcRepository::from_client(ReadServiceClient::new(channel))
        }

        #[tokio::test]
        async fn get_all_break_counts_maps_grpc_response_to_domain() {
            let result = connected_repository()
                .await
                .get_all_break_counts()
                .await
                .unwrap();

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].player.uuid.as_str().unwrap(), super::TEST_UUID);
            assert_eq!(result[0].break_count, 42);
        }

        #[tokio::test]
        async fn get_all_build_counts_maps_grpc_response_to_domain() {
            let result = connected_repository()
                .await
                .get_all_build_counts()
                .await
                .unwrap();

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].player.uuid.as_str().unwrap(), super::TEST_UUID);
            assert_eq!(result[0].build_count, 43);
        }

        #[tokio::test]
        async fn get_all_play_ticks_maps_grpc_response_to_domain() {
            let result = connected_repository()
                .await
                .get_all_play_ticks()
                .await
                .unwrap();

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].player.uuid.as_str().unwrap(), super::TEST_UUID);
            assert_eq!(result[0].play_ticks, 44);
        }

        #[tokio::test]
        async fn get_all_vote_counts_maps_grpc_response_to_domain() {
            let result = connected_repository()
                .await
                .get_all_vote_counts()
                .await
                .unwrap();

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].player.uuid.as_str().unwrap(), super::TEST_UUID);
            assert_eq!(result[0].vote_count, 45);
        }
    }
}
