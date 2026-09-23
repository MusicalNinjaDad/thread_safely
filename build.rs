use build_safely::prelude::*;

fn main() -> Result<()> {
    let mut ac = AutoCfg::new()?;

    // check to see any  downstream crate has defined
    // `unstable.allow-features` in `.cargo/config.toml`.
    let allowed_features = cargo_allowed_features()?;

    ac.emit_unstable_feature(try_blocks, &allowed_features);
    ac.emit_unstable_feature(try_trait_v2, &allowed_features);
    ac.emit_unstable_feature(try_trait_v2_residual, &allowed_features);

    Ok(())
}
