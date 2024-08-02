use processor::IndexerGrpcProcessorConfig;
use server_framework::RunnableConfig;
use std::io::Write;
use tokio::task::JoinSet;

const RUNTIME_WORKER_MULTIPLIER: usize = 2;

fn main() -> Result<(), anyhow::Error> {
	use tracing_subscriber::EnvFilter;

	tracing_subscriber::fmt()
		.with_env_filter(
			EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
		)
		.init();

	let dot_movement = dot_movement::DotMovement::try_from_env()?;
	let maptos_config =
		dot_movement.try_get_config_from_json::<maptos_execution_util::config::Config>()?;
	let default_indexer_config =
		build_processor_conf("default_processor", &maptos_config, &dot_movement)?;
	let usertx_indexer_config =
		build_processor_conf("user_transaction_processor", &maptos_config, &dot_movement)?;
	let accounttx_indexer_config =
		build_processor_conf("account_transactions_processor", &maptos_config, &dot_movement)?;
	let coin_indexer_config =
		build_processor_conf("coin_processor", &maptos_config, &dot_movement)?;
	let event_indexer_config =
		build_processor_conf("events_processor", &maptos_config, &dot_movement)?;
	let fungible_indexer_config =
		build_processor_conf("fungible_asset_processor", &maptos_config, &dot_movement)?;
	let txmeta_indexer_config =
		build_processor_conf("transaction_metadata_processor", &maptos_config, &dot_movement)?;

	let num_cpus = num_cpus::get();
	let worker_threads = (num_cpus * RUNTIME_WORKER_MULTIPLIER).max(16);
	println!(
		"[Processor] Starting processor tokio runtime: num_cpus={}, worker_threads={}",
		num_cpus, worker_threads
	);

	let mut builder = tokio::runtime::Builder::new_multi_thread();
	builder
		.disable_lifo_slot()
		.enable_all()
		.worker_threads(worker_threads)
		.build()
		.unwrap()
		.block_on({
			async move {
				let mut set = JoinSet::new();
				set.spawn(async move { default_indexer_config.run().await });
				//wait all the migration is done.
				tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
				set.spawn(async move { usertx_indexer_config.run().await });
				set.spawn(async move { accounttx_indexer_config.run().await });
				set.spawn(async move { coin_indexer_config.run().await });
				set.spawn(async move { event_indexer_config.run().await });
				set.spawn(async move { fungible_indexer_config.run().await });
				set.spawn(async move { txmeta_indexer_config.run().await });

				while let Some(res) = set.join_next().await {
					if let Err(err) = res {
						tracing::error!("An Error occurs during indexer execution: {err}");
					}
				}
				Ok(())
			}
		})
}

fn build_processor_conf(
	processor_name: &str,
	maptos_config: &maptos_execution_util::config::Config,
	dot_movement: &dot_movement::DotMovement,
) -> Result<IndexerGrpcProcessorConfig, anyhow::Error> {
	let indexer_grpc_data_service_address = format!(
		"http://{}:{}",
		maptos_config.indexer.maptos_indexer_grpc_listen_hostname,
		maptos_config.indexer.maptos_indexer_grpc_listen_port
	);
	tracing::info!(
		"Connecting to indexer gRPC server at: {}",
		indexer_grpc_data_service_address.clone()
	);
	//create config file
	let indexer_config_content = format!(
		"processor_config:
  type: {}
postgres_connection_string: {}/postgres
indexer_grpc_data_service_address: {}
indexer_grpc_http2_ping_interval_in_secs: 60
indexer_grpc_http2_ping_timeout_in_secs: 10
auth_token: \"{}\"",
		processor_name,
		maptos_config.indexer_processor.postgres_connection_string,
		indexer_grpc_data_service_address,
		maptos_config.indexer_processor.indexer_processor_auth_token,
	);

	let indexer_config_path = dot_movement.get_path().join("indexer_config.yaml");
	let mut output_file = std::fs::File::create(&indexer_config_path)?;
	write!(output_file, "{}", indexer_config_content)?;

	let indexer_config =
		server_framework::load::<IndexerGrpcProcessorConfig>(&indexer_config_path)?;
	Ok(indexer_config)
}
