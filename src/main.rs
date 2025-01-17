mod cli;
mod control;
#[cfg(windows)]
mod service;

use crate::cli::{evaluate_cli, Subcommand};
use log::{debug, error};

fn prepare_logging(
    name: &str,
    log_dir: Option<&String>,
    console: bool,
    rotation: cli::LogRotation,
    retention: usize,
    log_as: Option<&String>,
    log_cmd_as: Option<&String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut exe_dir = std::env::current_exe()?;
    exe_dir.pop();

    let log_dir = match log_dir {
        Some(log_dir) => log_dir.to_string(),
        None => exe_dir.to_string_lossy().to_string(),
    }
    .replace("\\\\?\\", "");

    let rotation = match rotation {
        cli::LogRotation::Bytes(bytes) => flexi_logger::Criterion::Size(bytes),
        cli::LogRotation::Daily => flexi_logger::Criterion::Age(flexi_logger::Age::Day),
        cli::LogRotation::Hourly => flexi_logger::Criterion::Age(flexi_logger::Age::Hour),
    };

    let mut logger = flexi_logger::Logger::try_with_env_or_str("debug")?
        .log_to_file({
            let spec = flexi_logger::FileSpec::default().directory(log_dir.clone());

            if let Some(log_as) = log_as {
                spec.basename(log_as)
            } else {
                spec.discriminant(format!("for_{}", name))
            }
        })
        .append()
        .rotate(
            rotation,
            flexi_logger::Naming::Timestamps,
            flexi_logger::Cleanup::KeepLogFiles(retention),
        )
        .format_for_files(|w, now, record| {
            write!(
                w,
                "{} [{}] {}",
                now.now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                &record.args()
            )
        })
        .format_for_stderr(|w, _now, record| write!(w, "[{}] {}", record.level(), &record.args()));

    if console {
        logger = logger.duplicate_to_stderr(flexi_logger::Duplicate::Info);
    }

    if let Some(log_cmd_as) = log_cmd_as {
        logger = logger.add_writer(
            "shawl-cmd",
            Box::new(
                flexi_logger::writers::FileLogWriter::builder(
                    flexi_logger::FileSpec::default()
                        .directory(log_dir)
                        .basename(log_cmd_as),
                )
                .append()
                .rotate(
                    rotation,
                    flexi_logger::Naming::Timestamps,
                    flexi_logger::Cleanup::KeepLogFiles(retention),
                )
                .format(|w, _now, record| write!(w, "{}", &record.args()))
                .try_build()?,
            ),
        );
    }

    logger.start()?;
    Ok(())
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = evaluate_cli();
    let console = !matches!(cli.sub, Subcommand::Run { .. });

    let should_log = match cli.clone().sub {
        Subcommand::Add { common: opts, .. } => !opts.no_log,
        Subcommand::Run { common: opts, .. } => !opts.no_log,
    };
    if should_log {
        let (name, common) = match &cli.sub {
            Subcommand::Add { name, common, .. } | Subcommand::Run { name, common, .. } => {
                (name, common)
            }
        };
        prepare_logging(
            name,
            common.log_dir.as_ref(),
            console,
            common.log_rotate.unwrap_or_default(),
            common.log_retain.unwrap_or(2),
            common.log_as.as_ref(),
            common.log_cmd_as.as_ref(),
        )?;
    }

    debug!("********** LAUNCH **********");
    debug!("{:?}", cli);

    match cli.sub {
        Subcommand::Add {
            name,
            cwd,
            dependencies,
            common: opts,
        } => match control::add_service(name, cwd, &dependencies, opts) {
            Ok(_) => (),
            Err(_) => std::process::exit(1),
        },
        Subcommand::Run { name, .. } => match service::run(name) {
            Ok(_) => (),
            Err(e) => {
                error!("Failed to run the service:\n{:#?}", e);
                // We wouldn't have a console if the Windows service manager
                // ran this, but if we failed here, then it's likely the user
                // tried to run it directly, so try showing them the error:
                println!("Failed to run the service:\n{:#?}", e);
                std::process::exit(1)
            }
        },
    }
    debug!("Finished successfully");
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    panic!("This program is only intended to run on Windows.");
}
