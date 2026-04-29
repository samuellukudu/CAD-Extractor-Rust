#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelToggle {
    Sidebar,
    Properties,
    CommandLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAction {
    Open,
    FitView,
    ClearSelection,
    SetAllLayers(bool),
    SetAllBlocks(bool),
    TogglePanel(PanelToggle),
}

pub fn parse_command(input: &str) -> Result<CommandAction, String> {
    let normalized = input.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err("Enter a command first.".to_owned());
    }

    let parts: Vec<&str> = normalized.split_whitespace().collect();
    match parts.as_slice() {
        ["open"] => Ok(CommandAction::Open),
        ["fit"] | ["fitview"] | ["zoom", "extents"] | ["zoom", "all"] => {
            Ok(CommandAction::FitView)
        }
        ["clear"] | ["clearselection"] | ["select", "none"] => Ok(CommandAction::ClearSelection),
        ["layers", "on"] | ["layer", "on"] => Ok(CommandAction::SetAllLayers(true)),
        ["layers", "off"] | ["layer", "off"] => Ok(CommandAction::SetAllLayers(false)),
        ["blocks", "on"] | ["block", "on"] => Ok(CommandAction::SetAllBlocks(true)),
        ["blocks", "off"] | ["block", "off"] => Ok(CommandAction::SetAllBlocks(false)),
        ["toggle", "sidebar"] => Ok(CommandAction::TogglePanel(PanelToggle::Sidebar)),
        ["toggle", "properties"] => Ok(CommandAction::TogglePanel(PanelToggle::Properties)),
        ["toggle", "command"] | ["toggle", "commandline"] => {
            Ok(CommandAction::TogglePanel(PanelToggle::CommandLine))
        }
        _ => Err(format!(
            "Unknown command: '{input}'. Try open, fit, clear, layers on/off, blocks on/off, or toggle sidebar/properties/command."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{CommandAction, PanelToggle, parse_command};

    #[test]
    fn parses_zoom_aliases() {
        assert_eq!(parse_command("fit").unwrap(), CommandAction::FitView);
        assert_eq!(parse_command("zoom extents").unwrap(), CommandAction::FitView);
        assert_eq!(parse_command("zoom all").unwrap(), CommandAction::FitView);
    }

    #[test]
    fn parses_visibility_commands() {
        assert_eq!(parse_command("layers on").unwrap(), CommandAction::SetAllLayers(true));
        assert_eq!(
            parse_command("blocks off").unwrap(),
            CommandAction::SetAllBlocks(false)
        );
    }

    #[test]
    fn parses_panel_toggles() {
        assert_eq!(
            parse_command("toggle sidebar").unwrap(),
            CommandAction::TogglePanel(PanelToggle::Sidebar)
        );
        assert_eq!(
            parse_command("toggle properties").unwrap(),
            CommandAction::TogglePanel(PanelToggle::Properties)
        );
    }

    #[test]
    fn rejects_unknown_commands() {
        assert!(parse_command("orbit").is_err());
    }
}
