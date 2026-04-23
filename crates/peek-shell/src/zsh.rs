/// Returns the zsh shell integration script.
pub fn init_script() -> &'static str {
    include_str!("../../../shell/peek.zsh")
}
