#![deny(clippy::all, clippy::cargo)]
#![warn(clippy::nursery, clippy::pedantic)]
#![allow(clippy::cargo_common_metadata)]

use std::sync::Arc;

pub(crate) mod domain {
    #[derive(Debug, Clone)]
    pub struct Player {
        pub uuid: String,
        pub last_known_name: String,
    }

    #[derive(Debug, Clone)]
    pub struct PlayerBreakCount {
        pub player: Option<Player>,
        pub break_count: u64,
    }

    #[derive(Debug, Clone)]
    pub struct PlayerBuildCount {
        pub player: Option<Player>,
        pub build_count: u64,
    }

    #[derive(Debug, Clone)]
    pub struct PlayerPlayTicks {
        pub player: Option<Player>,
        pub play_ticks: u64,
    }

    #[derive(Debug, Clone)]
    pub struct PlayerVoteCount {
        pub player: Option<Player>,
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

pub(crate) mod use_cases {
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

pub(crate) mod infra_axum_handlers {
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

pub(crate) mod infra_repository_impls {
    use crate::domain::{
        PlayerBreakCount, PlayerBuildCount, PlayerDataRepository, PlayerPlayTicks, PlayerVoteCount,
    };

    #[allow(dead_code)]
    #[allow(clippy::nursery, clippy::pedantic)]
    pub(crate) mod buf_generated {
        include!("gen/mod.rs");
    }

    pub struct SeichiGameDataGrpcClient {}

    #[async_trait::async_trait]
    impl PlayerDataRepository for SeichiGameDataGrpcClient {
        async fn get_all_break_counts(&self) -> anyhow::Result<Vec<PlayerBreakCount>> {
            // TODO: Call API
            Ok(vec![])
        }

        async fn get_all_build_counts(&self) -> anyhow::Result<Vec<PlayerBuildCount>> {
            // TODO: Call API
            Ok(vec![])
        }

        async fn get_all_play_ticks(&self) -> anyhow::Result<Vec<PlayerPlayTicks>> {
            // TODO: Call API
            Ok(vec![])
        }

        async fn get_all_vote_counts(&self) -> anyhow::Result<Vec<PlayerVoteCount>> {
            // TODO: Call API
            Ok(vec![])
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // initialize tracing
    tracing_subscriber::fmt::init();

    let repository = Arc::new(infra_repository_impls::SeichiGameDataGrpcClient {});

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
