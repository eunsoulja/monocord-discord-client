//! Personal Notes widget.

use egui::{RichText, Ui};

use l_core::models::{GridSize, PlacedWidget};
use l_ui::theme::color;

use crate::core::types::{
    DataNeed, Permission, RefreshPolicy, SettingField, SettingKind, WidgetCategory, WidgetCx,
    WidgetData, WidgetError, WidgetFactory, WidgetInstance, WidgetManifest,
};

static MANIFEST: std::sync::LazyLock<WidgetManifest> =
    std::sync::LazyLock::new(|| WidgetManifest {
        id: "l.personal_notes",
        name: "Personal Notes",
        description: "Local scratchpad persisted in the workspace database.",
        icon: "sticky-note",
        version: "1.0.0",
        category: WidgetCategory::Productivity,
        permissions: &[Permission::LocalNotes, Permission::LocalStorage],
        default_size: GridSize::new(4, 3),
        min_size: GridSize::new(3, 2),
        max_size: Some(GridSize::new(8, 8)),
        refresh: RefreshPolicy::Push,
        settings: vec![
            SettingField {
                key: "workspace_scope".into(),
                label: "Scope to workspace".into(),
                kind: SettingKind::Boolean,
                default: serde_json::json!(true),
            },
            SettingField {
                key: "note_title".into(),
                label: "Note title".into(),
                kind: SettingKind::Text,
                default: serde_json::json!("scratch"),
            },
        ],
    });

/// Payload.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct NotesPayload {
    /// Notes.
    pub notes: Vec<l_core::models::Note>,
}

pub struct PersonalNotesFactory;

impl WidgetFactory for PersonalNotesFactory {
    fn manifest(&self) -> &'static WidgetManifest {
        &MANIFEST
    }
    fn create(&self, placed: &PlacedWidget) -> Result<Box<dyn WidgetInstance>, WidgetError> {
        Ok(Box::new(NotesWidget {
            placed: placed.clone(),
            selected: None,
        }))
    }
}

struct NotesWidget {
    placed: PlacedWidget,
    selected: Option<uuid::Uuid>,
}

impl NotesWidget {
    fn title_filter(&self) -> String {
        crate::core::types::cfg_string(&self.placed, &MANIFEST, "note_title")
    }
}

impl WidgetInstance for NotesWidget {
    fn data_needs(&self) -> Vec<DataNeed> {
        // Workspace scoping is resolved by the app: `Some("")` means "current
        // workspace", `None` means all notes.
        let scope =
            crate::core::types::config_get::<bool>(&self.placed, &MANIFEST, "workspace_scope")
                .unwrap_or(true)
                .then(String::new);
        vec![DataNeed::Notes { workspace: scope }]
    }

    fn on_config_changed(&mut self, config: &serde_json::Value) {
        self.placed.config = config.clone();
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut WidgetCx<'_>, data: &WidgetData) {
        let Some(payload) = data.decode::<NotesPayload>() else {
            ui.label(
                RichText::new("loading notes…")
                    .small()
                    .color(color::text_muted()),
            );
            return;
        };
        let title_want = self.title_filter();
        let note = payload
            .notes
            .iter()
            .find(|n| n.title == title_want)
            .or_else(|| payload.notes.first());
        let Some(note) = note else {
            if ui.button("+ new note").clicked() {
                (cx.dispatch)(l_core::Action::NewNote);
            }
            ui.label(
                RichText::new("no notes yet")
                    .small()
                    .color(color::text_muted()),
            );
            return;
        };
        self.selected = Some(note.id);
        let mut body = note.body.clone();
        let resp = l_ui::components::field::multiline(ui, "note-body", &mut body, "note body…");
        if resp.changed() {
            (cx.dispatch)(l_core::Action::UpdateNote {
                id: note.id,
                field: l_core::actions::NoteField::Body,
                text: body,
            });
        }
        let mut t = note.title.clone();
        if l_ui::components::field::field(ui, "note-title", &mut t, "title", false).changed() {
            (cx.dispatch)(l_core::Action::UpdateNote {
                id: note.id,
                field: l_core::actions::NoteField::Title,
                text: t,
            });
        }
    }
}
