use crate::app::config::Config;
use eframe::egui::{self, Ui};

#[derive(Debug)]
pub enum ProfileManagerAction {
    Login(String),
    Add(String),
    Remove(String),
}

pub fn draw_profiles_ui(
    ctx: &egui::Context,
    ui: &mut Ui,
    config: &Config, // No longer mutable, actions will be handled in app_layout
    new_profile_name: &mut String,
    profile_to_remove_name: &mut String,
    error: &Option<String>,
) -> Option<ProfileManagerAction> {
    let mut action = None;

    ui.heading("Switch Profile");
    ui.separator();

    // List profiles with a "Switch" button
    for profile in &config.profiles {
        ui.horizontal(|ui| {
            let is_active = config.active_profile_name.as_ref() == Some(&profile.name);
            let label = if is_active {
                format!("{} (Active)", profile.name)
            } else {
                profile.name.clone()
            };
            ui.label(label);

            if !is_active {
                if ui.button("Switch").clicked() {
                    action = Some(ProfileManagerAction::Login(profile.name.clone()));
                }
            }
        });
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(10.0);

    // --- Add Profile Section ---
    ui.heading("Add New Profile");
    ui.horizontal(|ui| {
        ui.label("Name:");
        let response = ui.text_edit_singleline(new_profile_name);
        let add_button_clicked = ui.button("Add").clicked();
        let enter_pressed = response.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter));

        if (add_button_clicked || enter_pressed) && !new_profile_name.is_empty() {
            if !config.profiles.iter().any(|p| p.name == *new_profile_name) {
                action = Some(ProfileManagerAction::Add(new_profile_name.clone()));
                new_profile_name.clear();
            }
            // The error will be set in app_layout and displayed on the next frame.
        }
    });

    if let Some(err) = error {
        ui.colored_label(egui::Color32::RED, err);
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(10.0);

    // --- Remove Profile Section ---
    ui.heading("Remove Profile");

    // Don't show remove section if there are no profiles to remove
    if !config.profiles.is_empty() {
        ui.horizontal(|ui| {
            ui.label("Select Profile:");

            // If profile_to_remove_name is not a valid profile, reset it.
            if !config.profiles.iter().any(|p| p.name == *profile_to_remove_name) {
                if let Some(first_profile) = config.profiles.first() {
                    *profile_to_remove_name = first_profile.name.clone();
                }
            }

            egui::ComboBox::from_id_salt("remove_profile_combo")
                .selected_text(profile_to_remove_name.as_str())
                .show_ui(ui, |ui| {
                    for profile in &config.profiles {
                        ui.selectable_value(
                            profile_to_remove_name,
                            profile.name.clone(),
                            &profile.name,
                        );
                    }
                });

            let remove_button_clicked = ui.button("Remove").clicked();
            if remove_button_clicked && !profile_to_remove_name.is_empty() {
                action = Some(ProfileManagerAction::Remove(
                    profile_to_remove_name.clone(),
                ));
            }
        });
    } else {
        ui.label("No profiles to remove.");
    }

    action
}
