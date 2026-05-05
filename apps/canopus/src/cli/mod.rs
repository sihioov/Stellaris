use crate::core::{CanopusError, CanopusResult};

mod args;
mod commands;
mod finalize;
mod submit;

pub async fn run(args: Vec<String>) -> CanopusResult<()> {
    if args.len() < 2 {
        return Err(CanopusError::InvalidInput(usage()));
    }

    match args[1].as_str() {
        "submit" => submit::submit(&args[2..]).await,
        "project-register" => commands::project_register::project_register(&args[2..]),
        "work-intake" => commands::work_intake::work_intake(&args[2..]),
        "delivery-finalize" => commands::delivery_finalize::delivery_finalize(&args[2..]),
        "watch" => finalize::watch(&args[2..]).await,
        "finalize" => finalize::finalize(&args[2..]).await,
        "status" => commands::status_artifacts::status(&args[2..]),
        "artifacts" => commands::status_artifacts::artifacts(&args[2..]),
        _ => Err(CanopusError::InvalidInput(usage())),
    }
}

fn usage() -> String {
    "usage: canopus submit [--repo <path>] [--state <path>] [--agenda-id <id>] [--task-type <type>] <request> | canopus project-register --repo <path> --github-owner <owner> --github-repo <repo> --project-owner-kind <org|user> --project-owner <owner> [--create-github-repo] --json | canopus work-intake --repo <path> --registration <json-or-path> --task-id <id> --agenda-id <id> --request <text> [--project-sync off|best-effort|required] --json | canopus delivery-finalize [--repo <path>] [--discord-approved] [--github-ready] [--merge] [--deploy-required] --json | canopus watch [--repo <path>] [--state <path>] [--once] [tasks-path] | canopus finalize [--repo <path>] [--state <path>] (--agenda-id <id>|--task-id <id>)".to_string()
}
