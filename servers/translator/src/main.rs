#![deny(clippy::all, clippy::cargo)]
#![warn(clippy::nursery, clippy::pedantic)]
#![allow(clippy::cargo_common_metadata)]

mod domain {
    #[derive(Debug, Clone)]
    pub struct Player {
        pub uuid: String,
        pub last_known_name: String,
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
    pub trait PlayerDataRepository: Sync + Send + 'static {
        async fn get_all_break_counts(&self) -> anyhow::Result<Vec<PlayerBreakCount>>;
        async fn get_all_build_counts(&self) -> anyhow::Result<Vec<PlayerBuildCount>>;
        async fn get_all_play_ticks(&self) -> anyhow::Result<Vec<PlayerPlayTicks>>;
        async fn get_all_vote_counts(&self) -> anyhow::Result<Vec<PlayerVoteCount>>;
    }
}

mod use_cases {
    use crate::domain::{KnownPlayerData, PlayerDataRepository};
    use std::sync::Arc;

    #[derive(Clone)]
    pub struct GetAllPlayerDataUseCase {
        pub repository: Arc<dyn PlayerDataRepository>,
    }

    impl GetAllPlayerDataUseCase {
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
    use axum::handler::Handler;
    use axum::http::StatusCode;
    use std::sync::Arc;

    mod presenter {
        use crate::domain::KnownPlayerData;

        pub fn present_player_data(_data: &KnownPlayerData) -> String {
            // TODO: present player_data using Prometheus v2 style and return as response
            "".to_string()
        }
    }

    pub fn handle_get_metrics(repository: &Arc<impl PlayerDataRepository>) -> impl Handler<()> {
        let use_case = GetAllPlayerDataUseCase {
            repository: repository.clone(),
        };

        || async move {
            match use_case.get_all_known_player_data().await {
                Ok(data) => (StatusCode::OK, presenter::present_player_data(&data)),
                Err(e) => {
                    tracing::error!("{:?}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Encountered internal server error. Please contact the server administrator to resolve the issue.".to_string(),
                    )
                }
            }
        }
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
            domain::Player {
                uuid: p.uuid,
                last_known_name: p.last_known_name,
            }
        }

        fn extract_domain_player(
            player: Option<generated::Player>,
        ) -> anyhow::Result<domain::Player> {
            let player = player.ok_or_else(|| anyhow::anyhow!("Player field not set"))?;

            Ok(into_domain_player(player))
        }

        pub fn try_into_domain_player_break_count(
            value: generated::PlayerBreakCount,
        ) -> anyhow::Result<domain::PlayerBreakCount> {
            Ok(domain::PlayerBreakCount {
                player: extract_domain_player(value.player)?,
                break_count: value.break_count,
            })
        }

        pub fn try_into_domain_player_build_count(
            value: generated::PlayerBuildCount,
        ) -> anyhow::Result<domain::PlayerBuildCount> {
            Ok(domain::PlayerBuildCount {
                player: extract_domain_player(value.player)?,
                build_count: value.build_count,
            })
        }

        pub fn try_into_domain_player_play_ticks(
            value: generated::PlayerPlayTicks,
        ) -> anyhow::Result<domain::PlayerPlayTicks> {
            Ok(domain::PlayerPlayTicks {
                player: extract_domain_player(value.player)?,
                play_ticks: value.play_ticks,
            })
        }

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

    pub struct SeichiGameDataGrpcClient {
        client: GameDataGrpcClient,
    }

    impl SeichiGameDataGrpcClient {
        pub async fn try_connect_with(
            config: config::GrpcClientConfig,
        ) -> anyhow::Result<SeichiGameDataGrpcClient> {
            let client =
                GameDataGrpcClient::connect(config.game_data_server_grpc_endpoint_url).await?;

            Ok(SeichiGameDataGrpcClient { client })
        }

        pub(crate) fn cloned_inner(&self) -> GameDataGrpcClient {
            self.client.clone()
        }
    }

    fn empty_request() -> tonic::Request<pbjson_types::Empty> {
        tonic::Request::new(pbjson_types::Empty::default())
    }

    use crate::domain::{PlayerBreakCount, PlayerBuildCount, PlayerPlayTicks, PlayerVoteCount};

    #[async_trait::async_trait]
    impl crate::domain::PlayerDataRepository for SeichiGameDataGrpcClient {
        async fn get_all_break_counts(&self) -> anyhow::Result<Vec<PlayerBreakCount>> {
            Ok(self
                .cloned_inner()
                .break_counts(empty_request())
                .await?
                .into_inner()
                .results
                .into_iter()
                .map(buf_generated_to_domain::try_into_domain_player_break_count)
                .collect::<Result<_, _>>()?)
        }

        async fn get_all_build_counts(&self) -> anyhow::Result<Vec<PlayerBuildCount>> {
            Ok(self
                .cloned_inner()
                .build_counts(empty_request())
                .await?
                .into_inner()
                .results
                .into_iter()
                .map(buf_generated_to_domain::try_into_domain_player_build_count)
                .collect::<Result<_, _>>()?)
        }

        async fn get_all_play_ticks(&self) -> anyhow::Result<Vec<PlayerPlayTicks>> {
            Ok(self
                .cloned_inner()
                .play_ticks(empty_request())
                .await?
                .into_inner()
                .results
                .into_iter()
                .map(buf_generated_to_domain::try_into_domain_player_play_ticks)
                .collect::<Result<_, _>>()?)
        }

        async fn get_all_vote_counts(&self) -> anyhow::Result<Vec<PlayerVoteCount>> {
            Ok(self
                .cloned_inner()
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
    use crate::infra_repository_impls;
    use std::sync::Arc;

    pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
        // initialize tracing
        tracing_subscriber::fmt::init();

        let repository = {
            let client_config = infra_repository_impls::config::GrpcClientConfig::from_env()?;
            let client =
                infra_repository_impls::SeichiGameDataGrpcClient::try_connect_with(client_config)
                    .await?;

            Arc::new(client)
        };

        let app = {
            use infra_axum_handlers::handle_get_metrics;

            use axum::routing::get;
            use axum::Router;

            Router::new().route("/metrics", get(handle_get_metrics(&repository)))
        };

        let addr = {
            use std::net::SocketAddr;
            SocketAddr::from(([0, 0, 0, 0], 80))
        };

        tracing::debug!("listening on {}", addr);

        Ok(axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await?)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    app::main().await
}
