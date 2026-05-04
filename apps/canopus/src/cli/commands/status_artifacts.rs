use crate::core::{CanopusError, CanopusResult};

pub(crate) fn status(args: &[String]) -> CanopusResult<()> {
    if args.len() != 1 {
        return Err(CanopusError::InvalidInput(
            "usage: canopus status <task-id>".to_string(),
        ));
    }
    println!("{}: local status is file-backed in MVP", args[0]);
    Ok(())
}

pub(crate) fn artifacts(args: &[String]) -> CanopusResult<()> {
    if args.len() != 1 {
        return Err(CanopusError::InvalidInput(
            "usage: canopus artifacts <task-id>".to_string(),
        ));
    }
    println!("artifacts for {} are under .canopus/artifacts", args[0]);
    Ok(())
}
