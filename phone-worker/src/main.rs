use phone_worker::{Worker, WorkerConfig};
#[tokio::main]
async fn main() {
    if std::env::args().any(|v| v == "healthcheck") {
        std::process::exit(if phone_worker::pipeline::healthcheck() {
            0
        } else {
            1
        });
    }
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();
    let result = async {
        let worker = Worker::new(WorkerConfig::from_env()?)?;
        if std::env::args().any(|v| v == "--once") {
            worker.run_once().await.map(|_| ())
        } else {
            worker.run().await
        }
    }
    .await;
    if let Err(error) = result {
        eprintln!("worker_error:{}", error.code);
        std::process::exit(1);
    }
}
