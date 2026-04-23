/// Returns the fish shell integration script.
pub fn init_script() -> &'static str {
    include_str!("../../../shell/peek.fish")
}
