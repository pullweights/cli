use anyhow::Result;

use crate::config::CliConfig;

pub fn logout() -> Result<()> {
    let mut cfg = CliConfig::load()?;
    if cfg.token.is_none() {
        println!("Not logged in.");
        return Ok(());
    }
    cfg.token = None;
    cfg.save()?;
    println!("Logged out.");
    Ok(())
}
