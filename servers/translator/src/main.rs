#![deny(clippy::all, clippy::cargo)]
#![warn(clippy::nursery, clippy::pedantic)]
#![allow(clippy::cargo_common_metadata)]

mod domain {
    use std::fmt::Debug;

    #[derive(Debug, Clone)]
    pub struct Player {
        pub uuid: String,
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

    #[derive(Debug, Clone)]
    pub struct KnownPlayerData {
        pub break_counts: Vec<PlayerBreakCount>,
        pub build_counts: Vec<PlayerBuildCount>,
        pub play_ticks: Vec<PlayerPlayTicks>,
        pub vote_counts: Vec<PlayerVoteCount>,
    }

    #[async_trait::async_trait]
    pub trait PlayerDataRepository: Debug + Sync + Send + 'static {
        async fn get_all_break_counts(&self) -> anyhow::Result<Vec<PlayerBreakCount>>;
        async fn get_all_build_counts(&self) -> anyhow::Result<Vec<PlayerBuildCount>>;
        async fn get_all_play_ticks(&self) -> anyhow::Result<Vec<PlayerPlayTicks>>;
        async fn get_all_vote_counts(&self) -> anyhow::Result<Vec<PlayerVoteCount>>;
    }
}

mod use_cases {
    use crate::domain::{KnownPlayerData, PlayerDataRepository};
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    pub struct GetAllPlayerDataUseCase {
        pub repository: Arc<dyn PlayerDataRepository>,
    }

    impl GetAllPlayerDataUseCase {
        #[tracing::instrument]
        pub async fn get_all_known_player_data(&self) -> anyhow::Result<KnownPlayerData> {
            Ok(KnownPlayerData {
                break_counts: self.repository.get_all_break_counts().await?,
                build_counts: self.repository.get_all_build_counts().await?,
                play_ticks: self.repository.get_all_play_ticks().await?,
                vote_counts: self.repository.get_all_vote_counts().await?,
            })
        }
    }
}

mod infra_axum_handlers {
    use crate::domain::PlayerDataRepository;
    use crate::use_cases::GetAllPlayerDataUseCase;
    use axum::body;
    use axum::handler::Handler;
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use std::sync::Arc;

    #[derive(Clone, Debug)]
    pub struct SharedAppState {
        pub repository: Arc<dyn PlayerDataRepository>,
    }

    mod presenter {
        use crate::domain::KnownPlayerData;
        use prometheus::core::{Collector, Desc};
        use prometheus::proto::MetricFamily;
        use prometheus::{Encoder, IntGaugeVec, Opts, TextEncoder};
        use std::collections::HashMap;

        struct Collectors {
            break_count: IntGaugeVec,
            build_count: IntGaugeVec,
            vote_count: IntGaugeVec,
            play_ticks: IntGaugeVec,
        }

        impl Collectors {
            fn new() -> anyhow::Result<Self> {
                let break_count =
                    IntGaugeVec::new(Opts::new("seichi_player_break_count", ""), &["uuid"])?;
                let build_count =
                    IntGaugeVec::new(Opts::new("seichi_player_build_count", ""), &["uuid"])?;
                let vote_count =
                    IntGaugeVec::new(Opts::new("seichi_player_vote_count", ""), &["uuid"])?;
                let play_ticks =
                    IntGaugeVec::new(Opts::new("seichi_player_play_ticks", ""), &["uuid"])?;

                Ok(Collectors {
                    break_count,
                    build_count,
                    vote_count,
                    play_ticks,
                })
            }
        }

        impl Collector for Collectors {
            fn desc(&self) -> Vec<&Desc> {
                vec![
                    self.break_count.desc(),
                    self.build_count.desc(),
                    self.vote_count.desc(),
                    self.play_ticks.desc(),
                ]
                .into_iter()
                .flatten()
                .collect()
            }

            fn collect(&self) -> Vec<MetricFamily> {
                vec![
                    self.break_count.collect(),
                    self.build_count.collect(),
                    self.vote_count.collect(),
                    self.play_ticks.collect(),
                ]
                .into_iter()
                .flatten()
                .collect()
            }
        }

        #[tracing::instrument]
        pub fn present_player_data_as_prometheus_metrics(
            data: &KnownPlayerData,
        ) -> anyhow::Result<String> {
            let collectors = Collectors::new()?;

            for record in &data.break_counts {
                let uuid = record.player.uuid.as_str();
                let metrics = collectors
                    .break_count
                    .with(&HashMap::from([("uuid", uuid)]));
                metrics.set(record.break_count as i64);
            }

            for record in &data.build_counts {
                let uuid = record.player.uuid.as_str();
                let metrics = collectors
                    .build_count
                    .with(&HashMap::from([("uuid", uuid)]));
                metrics.set(record.build_count as i64);
            }

            for record in &data.vote_counts {
                let uuid = record.player.uuid.as_str();
                let metrics = collectors.vote_count.with(&HashMap::from([("uuid", uuid)]));
                metrics.set(record.vote_count as i64);
            }

            for record in &data.play_ticks {
                let uuid = record.player.uuid.as_str();
                let metrics = collectors.play_ticks.with(&HashMap::from([("uuid", uuid)]));
                metrics.set(record.play_ticks as i64);
            }

            let mut buffer = vec![];

            TextEncoder::new().encode(&collectors.collect(), &mut buffer)?;

            Ok(String::from_utf8(buffer)?)
        }
    }

    fn const_error_response() -> (StatusCode, Response) {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Response::new(
                body::boxed("Encountered internal server error. Please contact the server administrator to resolve the issue.".to_string())),
        )
    }

    pub fn handle_get_metrics(state: SharedAppState) -> impl Handler<()> {
        // we need a separate handler function to create an error tracing span
        #[tracing::instrument]
        async fn handler(state: &SharedAppState) -> Response {
            let use_case = GetAllPlayerDataUseCase {
                repository: state.repository.clone(),
            };

            match use_case
                .get_all_known_player_data()
                .await
                .and_then(|known_player_data| {
                    presenter::present_player_data_as_prometheus_metrics(&known_player_data)
                }) {
                Ok(metrics_presentation) => {
                    (StatusCode::OK, Response::new(metrics_presentation)).into_response()
                }
                Err(e) => {
                    tracing::error!("{:?}", e);
                    const_error_response().into_response()
                }
            }
        }

        || async move { handler(&state).await }
    }
}

mod infra_repository_impls {
    #[allow(dead_code)]
    #[allow(clippy::nursery, clippy::pedantic)]
    mod buf_generated {
        include!("gen/mod.rs");
    }

    pub mod config {
        #[derive(serde::Deserialize, Debug, Clone)]
        pub struct GrpcClientConfig {
            pub game_data_server_grpc_endpoint_url: String,
        }

        impl GrpcClientConfig {
            pub fn from_env() -> anyhow::Result<GrpcClientConfig> {
                Ok(envy::from_env::<GrpcClientConfig>()?)
            }
        }
    }

    mod buf_generated_to_domain {
        use super::buf_generated::gigantic_minecraft::seichi_game_data::v1 as generated;
        use crate::domain;

        fn into_domain_player(p: generated::Player) -> domain::Player {
            domain::Player { uuid: p.uuid }
        }

        #[tracing::instrument]
        fn extract_domain_player(
            player: Option<generated::Player>,
        ) -> anyhow::Result<domain::Player> {
            let player = player.ok_or_else(|| anyhow::anyhow!("Player field not set"))?;

            Ok(into_domain_player(player))
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
        ) -> anyhow::Result<GameDataGrpcRepository> {
            let client =
                GameDataGrpcClient::connect(config.game_data_server_grpc_endpoint_url).await?;

            Ok(GameDataGrpcRepository { client })
        }

        pub(crate) fn game_data_client(&self) -> GameDataGrpcClient {
            self.client.clone()
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

        let app = {
            use infra_axum_handlers::handle_get_metrics;

            use axum::routing::get;
            use axum::Router;

            Router::new()
                .route("/metrics", get(handle_get_metrics(shared_state.clone())))
                .layer(TraceLayer::new_for_http())
        };

        let addr = {
            use std::net::SocketAddr;
            SocketAddr::from(([0, 0, 0, 0], 80))
        };

        tracing::info!("listening on {}", addr);

        Ok(axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await?)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    app::main().await
}
