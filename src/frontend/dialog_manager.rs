//! Centralized dialog state management.

use super::dialogs::{
    CollectionSettingsState, ConnectionSettingsState, DuplicateConfirmState, ElfSymbolsState,
    PersistenceSettingsState, PreferencesState, VariableChangeState,
};

/// Manages all dialog open/close state and per-dialog data.
pub struct DialogManager {
    pub variable_change: (bool, VariableChangeState),
    pub elf_symbols: (bool, ElfSymbolsState),
    pub duplicate_confirm: (bool, DuplicateConfirmState),
    pub connection_settings: (bool, ConnectionSettingsState),
    pub collection_settings: (bool, CollectionSettingsState),
    pub persistence_settings: (bool, PersistenceSettingsState),
    pub preferences: (bool, PreferencesState),
    pub connection_dialog: bool,
    pub help: bool,
}

impl DialogManager {
    pub fn new() -> Self {
        Self {
            variable_change: (false, VariableChangeState::default()),
            elf_symbols: (false, ElfSymbolsState::default()),
            duplicate_confirm: (false, DuplicateConfirmState::default()),
            connection_settings: (false, ConnectionSettingsState::default()),
            collection_settings: (false, CollectionSettingsState::default()),
            persistence_settings: (false, PersistenceSettingsState::default()),
            preferences: (false, PreferencesState::default()),
            connection_dialog: false,
            help: false,
        }
    }

    pub fn close_all(&mut self) {
        self.variable_change.0 = false;
        self.elf_symbols.0 = false;
        self.duplicate_confirm.0 = false;
        self.connection_settings.0 = false;
        self.collection_settings.0 = false;
        self.persistence_settings.0 = false;
        self.preferences.0 = false;
        self.connection_dialog = false;
        self.help = false;
    }
}

impl Default for DialogManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initially_all_closed() {
        let dm = DialogManager::new();
        assert!(!dm.variable_change.0);
        assert!(!dm.elf_symbols.0);
        assert!(!dm.duplicate_confirm.0);
        assert!(!dm.connection_settings.0);
        assert!(!dm.collection_settings.0);
        assert!(!dm.persistence_settings.0);
        assert!(!dm.preferences.0);
        assert!(!dm.connection_dialog);
        assert!(!dm.help);
    }

    #[test]
    fn test_open_close() {
        let mut dm = DialogManager::new();
        dm.elf_symbols.0 = true;
        assert!(dm.elf_symbols.0);
        dm.elf_symbols.0 = false;
        assert!(!dm.elf_symbols.0);
    }

    #[test]
    fn test_close_all() {
        let mut dm = DialogManager::new();
        dm.variable_change.0 = true;
        dm.elf_symbols.0 = true;
        dm.preferences.0 = true;
        dm.help = true;
        dm.close_all();
        assert!(!dm.variable_change.0);
        assert!(!dm.elf_symbols.0);
        assert!(!dm.preferences.0);
        assert!(!dm.help);
    }

    #[test]
    fn test_dialogs_independent() {
        let mut dm = DialogManager::new();
        dm.elf_symbols.0 = true;
        assert!(!dm.variable_change.0);
        assert!(!dm.collection_settings.0);
    }
}
