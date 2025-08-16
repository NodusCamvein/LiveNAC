use crate::app::config::Config;
use eframe::egui;

#[derive(Debug)]
pub enum ProfileManagerAction {
    Login(String),
    Add(String),
    Remove(String),
}

pub fn draw_profile_manager(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    config: &mut Config,
    new_profile_name: &mut String,
) -> Option<ProfileManagerAction> {
    let mut action = None;

    ui.heading("Profiles");
    ui.separator();

    for profile in &config.profiles {
        ui.horizontal(|ui| {
            ui.label(&profile.name);
            if ui.button("Login").clicked() {
                action = Some(ProfileManagerAction::Login(profile.name.clone()));
            }
            if ui.button("Delete").clicked() {
                action = Some(ProfileManagerAction::Remove(profile.name.clone()));
            }
        });
    }

    ui.separator();
    ui.horizontal(|ui| {
        ui.label("New Profile Name:");
        let response = ui.text_edit_singleline(new_profile_name);

        let enter_pressed = response.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter));
        let add_button_clicked = ui.button("Add").clicked();

        if (add_button_clicked || enter_pressed) && !new_profile_name.is_empty() {
            action = Some(ProfileManagerAction::Add(new_profile_name.clone()));
            new_profile_name.clear();
        }
    });

    action
}
