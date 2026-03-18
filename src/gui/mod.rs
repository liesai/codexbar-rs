use crate::backend::{
    self, BackendDoctorInput, BackendDoctorOutput, BackendStatusInput, BackendStatusOutput,
    get_config_path,
};
use crate::providers::{SourceMode, UsageSnapshot};
use eframe::egui;
use std::collections::BTreeMap;
use tokio::runtime::Runtime;

pub struct CodexBarGuiApp {
    runtime: Runtime,
    selected_source: SourceMode,
    selected_provider: String,
    no_cache: bool,
    status_output: Option<BackendStatusOutput>,
    doctor_output: Option<BackendDoctorOutput>,
    config_path: String,
    last_error: Option<String>,
}

impl CodexBarGuiApp {
    pub fn new(runtime: Runtime) -> Self {
        let config_path = get_config_path()
            .map(|output| output.config_path.display().to_string())
            .unwrap_or_else(|error| format!("failed to resolve config path: {error}"));

        let mut app = Self {
            runtime,
            selected_source: SourceMode::Auto,
            selected_provider: String::from("all"),
            no_cache: false,
            status_output: None,
            doctor_output: None,
            config_path,
            last_error: None,
        };

        app.refresh();
        app
    }

    fn refresh(&mut self) {
        self.last_error = None;

        let provider = if self.selected_provider == "all" {
            None
        } else {
            Some(self.selected_provider.clone())
        };

        let status_input = BackendStatusInput {
            source: Some(self.selected_source),
            provider,
            refresh: true,
            no_cache: self.no_cache,
        };
        let doctor_input = BackendDoctorInput {
            source: Some(self.selected_source),
        };

        match self.runtime.block_on(backend::get_status(status_input)) {
            Ok(output) => self.status_output = Some(output),
            Err(error) => self.last_error = Some(format!("status refresh failed: {error}")),
        }

        match backend::get_doctor(doctor_input) {
            Ok(output) => self.doctor_output = Some(output),
            Err(error) => {
                self.last_error = Some(match &self.last_error {
                    Some(existing) => format!("{existing}; doctor refresh failed: {error}"),
                    None => format!("doctor refresh failed: {error}"),
                })
            }
        }
    }

    fn provider_options(&self) -> Vec<String> {
        let mut options = vec![String::from("all")];
        options.extend(
            backend::get_provider_names()
                .iter()
                .map(|provider| (*provider).to_string()),
        );
        options
    }

    fn render_status_cards(&self, ui: &mut egui::Ui, providers: &BTreeMap<String, UsageSnapshot>) {
        for (name, snapshot) in providers {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.heading(name);
                ui.label(format!("health: {:?}", snapshot.health));
                ui.label(format!("source: {:?}", snapshot.source));
                ui.label(format!("stale: {}", snapshot.stale));
                if let Some(account) = &snapshot.account {
                    ui.label(format!("account: {account}"));
                }
                if let Some(plan) = &snapshot.plan {
                    ui.label(format!("plan: {plan}"));
                }
                if let Some(auth_mode) = &snapshot.auth_mode {
                    ui.label(format!("auth_mode: {auth_mode}"));
                }

                if let Some(used) = snapshot.primary.used {
                    ui.label(format!("primary.used: {used}"));
                }
                if let Some(limit) = snapshot.primary.limit {
                    ui.label(format!("primary.limit: {limit}"));
                }
                if let Some(remaining) = snapshot.primary.remaining {
                    ui.label(format!("primary.remaining: {remaining}"));
                }

                if let Some(secondary) = &snapshot.secondary {
                    if let Some(used) = secondary.used {
                        ui.label(format!("secondary.used: {used}"));
                    }
                    if let Some(limit) = secondary.limit {
                        ui.label(format!("secondary.limit: {limit}"));
                    }
                }

                if let Some(prompt_tokens) = snapshot.prompt_tokens {
                    ui.label(format!("prompt_tokens: {prompt_tokens}"));
                }
                if let Some(completion_tokens) = snapshot.completion_tokens {
                    ui.label(format!("completion_tokens: {completion_tokens}"));
                }
                if let Some(total_tokens) = snapshot.total_tokens {
                    ui.label(format!("total_tokens: {total_tokens}"));
                }
                if let Some(error) = &snapshot.error {
                    ui.colored_label(egui::Color32::YELLOW, format!("error: {error}"));
                }
            });
            ui.add_space(8.0);
        }
    }
}

impl eframe::App for CodexBarGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Provider");
                egui::ComboBox::from_id_salt("provider-select")
                    .selected_text(&self.selected_provider)
                    .show_ui(ui, |ui| {
                        for provider in self.provider_options() {
                            ui.selectable_value(
                                &mut self.selected_provider,
                                provider.clone(),
                                provider,
                            );
                        }
                    });

                ui.label("Source");
                egui::ComboBox::from_id_salt("source-select")
                    .selected_text(self.selected_source.as_str())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.selected_source, SourceMode::Auto, "auto");
                        ui.selectable_value(&mut self.selected_source, SourceMode::Api, "api");
                        ui.selectable_value(&mut self.selected_source, SourceMode::Cli, "cli");
                    });

                ui.checkbox(&mut self.no_cache, "No cache");

                if ui.button("Refresh").clicked() {
                    self.refresh();
                }
            });
        });

        egui::SidePanel::right("diagnostics")
            .resizable(true)
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.heading("Doctor");
                ui.label(format!("Config path: {}", self.config_path));
                ui.add_space(8.0);

                if let Some(error) = &self.last_error {
                    ui.colored_label(egui::Color32::RED, error);
                    ui.add_space(8.0);
                }

                if let Some(report) = &self.doctor_output {
                    let json = serde_json::to_string_pretty(report)
                        .unwrap_or_else(|err| format!("failed to serialize doctor report: {err}"));
                    ui.code(json);
                } else {
                    ui.label("No doctor report loaded.");
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Status");
            ui.add_space(8.0);

            if let Some(output) = &self.status_output {
                self.render_status_cards(ui, &output.providers);
            } else {
                ui.label("No status loaded.");
            }
        });
    }
}
