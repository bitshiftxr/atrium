use super::{Daemon, Outcome};
use crate::ha::action;
use crate::ha::client::{CommandWriter, Error as ClientError};
use crate::ha::url;
use crate::keyring;
use crate::protocol::{Command, ConnectionState, Level};

impl Daemon {
    pub(super) async fn handle_offline(&mut self, command: Command) -> Outcome {
        match command {
            Command::Connect | Command::Refresh => Outcome::Reconfigure,
            Command::Disconnect => {
                self.emit_status(ConnectionState::Offline, None);
                Outcome::Lost(ClientError::Closed)
            }
            Command::SetUrl { url: new_url } => self.set_url(new_url),
            Command::SetToken { token, url } => self.set_token(token, url).await,
            Command::ForgetToken => self.forget_token().await,
            Command::SetAreaPrefs {
                order,
                hidden,
                hide_empty_areas,
                hide_entities_without_area,
            } => self.set_area_prefs(order, hidden, hide_empty_areas, hide_entities_without_area),
            Command::SetFavorites { ids, show } => {
                self.config.favorites = ids;
                if let Some(value) = show {
                    self.config.show_favorites = value;
                }
                self.save();
                self.emit_config();
                Outcome::Lost(ClientError::Closed)
            }
            Command::SetPinnedTabs { ids } => {
                self.config.pinned_tabs = ids;
                self.save();
                self.emit_config();
                Outcome::Lost(ClientError::Closed)
            }
            Command::SetSelectedTab { tab } => {
                self.config.selected_tab = tab;
                self.save();
                Outcome::Lost(ClientError::Closed)
            }
            Command::Status => {
                self.emit_config();
                self.emit_projection();
                self.replay_history();
                Outcome::Lost(ClientError::Closed)
            }
            Command::Action { entity_id, .. } => {
                self.log(
                    Level::Warn,
                    "Not connected to Home Assistant.",
                    Some(entity_id),
                );
                Outcome::Lost(ClientError::Closed)
            }
            Command::AreaPower { .. } => {
                self.warn("Not connected to Home Assistant.");
                Outcome::Lost(ClientError::Closed)
            }
            Command::ImportDashboardPrefs => {
                self.warn("Not connected to Home Assistant.");
                Outcome::Lost(ClientError::Closed)
            }
        }
    }

    fn set_url(&mut self, new_url: String) -> Outcome {
        if let Err(e) = url::parse(&new_url) {
            self.warn(e.to_string());
            return Outcome::Lost(ClientError::Closed);
        }
        self.config.base_url = new_url;
        self.save();
        self.emit_config();
        Outcome::Reconfigure
    }

    /// The address comes from this command, never from stored config: a token
    /// typed for one server must not be stored under, or sent to, whichever
    /// server happens to be configured.
    async fn set_token(&mut self, token: String, url: Option<String>) -> Outcome {
        let target = url.as_deref().unwrap_or(&self.config.base_url);
        let endpoint = match url::parse(target) {
            Ok(endpoint) => endpoint,
            Err(e) => {
                self.error(e.to_string());
                return Outcome::Lost(ClientError::Closed);
            }
        };

        let token = token.trim();
        if token.is_empty() {
            self.warn("That token is empty.");
            return Outcome::Lost(ClientError::Closed);
        }

        if let Err(e) = keyring::store(&endpoint.origin, token).await {
            self.error(e.to_string());
            return Outcome::Lost(ClientError::Closed);
        }

        if self.config.base_url != target {
            self.config.base_url = target.to_string();
            self.save();
            self.emit_config();
        }
        Outcome::Reconfigure
    }

    async fn forget_token(&mut self) -> Outcome {
        if let Ok(endpoint) = url::parse(&self.config.base_url) {
            if let Err(e) = keyring::clear(&endpoint.origin).await {
                self.error(e.to_string());
            }
        }
        Outcome::Reconfigure
    }

    /// An absent field means "leave it alone", so an update that carries only
    /// one setting cannot clear the others.
    fn set_area_prefs(
        &mut self,
        order: Option<Vec<String>>,
        hidden: Option<Vec<String>>,
        hide_empty_areas: Option<bool>,
        hide_entities_without_area: Option<bool>,
    ) -> Outcome {
        let areas = &mut self.config.areas;
        if let Some(order) = order {
            areas.order = order;
        }
        if let Some(hidden) = hidden {
            areas.hidden = hidden;
        }
        if let Some(value) = hide_empty_areas {
            areas.hide_empty_areas = value;
        }
        if let Some(value) = hide_entities_without_area {
            areas.hide_entities_without_area = value;
        }
        self.config.imported_dashboard_prefs = true;
        self.save();
        self.emit_projection();
        self.emit_config();
        Outcome::Lost(ClientError::Closed)
    }

    /// The calls that switch every plain on/off device a tab shows, and the
    /// tab's title. The tab is re-projected here rather than trusted from the
    /// panel, so the set is exactly what is drawn now and never includes a lock.
    fn area_power_calls(
        &self,
        tab_id: &str,
        on: bool,
    ) -> Result<(String, Vec<action::BulkCall>), String> {
        let Some(tab) = self.tabs().into_iter().find(|t| t.id == tab_id) else {
            return Err(format!("There is no room called {tab_id}."));
        };
        let states = tab.entity_ids.iter().filter_map(|id| {
            let state = self.states.get(id)?.get("state")?.as_str()?;
            Some((id.as_str(), state))
        });
        let calls = action::area_power(states, on);
        if calls.is_empty() {
            return Err(format!(
                "{} has nothing to switch {}.",
                tab.title,
                if on { "on" } else { "off" }
            ));
        }
        Ok((tab.title, calls))
    }

    async fn area_power(
        &mut self,
        tab_id: &str,
        on: bool,
        writer: &mut CommandWriter,
    ) -> Option<Outcome> {
        let (title, calls) = match self.area_power_calls(tab_id, on) {
            Ok(found) => found,
            Err(message) => {
                self.warn(message);
                return None;
            }
        };
        let mut sent = Vec::with_capacity(calls.len());
        for call in &calls {
            if let Err(e) = writer
                .call_service_many(&call.domain, call.service, &call.entity_ids)
                .await
            {
                return Some(Outcome::Lost(e));
            }
            sent.push(format!(
                "{}.{} ×{}",
                call.domain,
                call.service,
                call.entity_ids.len()
            ));
        }
        self.log(
            Level::Info,
            format!(
                "{title} — all {}: {}",
                if on { "on" } else { "off" },
                sent.join(", ")
            ),
            None,
        );
        None
    }

    pub(super) async fn handle_online(
        &mut self,
        command: Command,
        writer: &mut CommandWriter,
    ) -> Option<Outcome> {
        match command {
            Command::Action {
                entity_id,
                action,
                data,
            } => {
                match action::resolve(&entity_id, &action, &data) {
                    Ok(resolved) => {
                        if let Err(e) = writer
                            .call_service(
                                &resolved.domain,
                                &resolved.service,
                                &resolved.entity_id,
                                resolved.data,
                            )
                            .await
                        {
                            return Some(Outcome::Lost(e));
                        }
                        self.log(
                            Level::Info,
                            format!(
                                "{} — {}.{}",
                                self.display_name(&entity_id),
                                resolved.domain,
                                resolved.service
                            ),
                            Some(entity_id.clone()),
                        );
                    }
                    Err(e) => self.log(Level::Warn, format!("{e}"), Some(entity_id.clone())),
                }
                None
            }
            Command::AreaPower { tab, on } => self.area_power(&tab, on, writer).await,
            Command::Refresh => Some(Outcome::Reconfigure),
            Command::Disconnect => {
                self.emit_status(ConnectionState::Offline, None);
                Some(Outcome::Stop)
            }
            Command::ImportDashboardPrefs => {
                self.info("Re-reading your Home Assistant dashboard.");
                self.config.imported_dashboard_prefs = false;
                self.save();
                Some(Outcome::Reconfigure)
            }
            Command::Status => {
                self.emit_config();
                self.emit_projection();
                self.replay_history();
                None
            }
            other => match self.handle_offline(other).await {
                Outcome::Stop => Some(Outcome::Stop),
                Outcome::Reconfigure => Some(Outcome::Reconfigure),
                Outcome::Lost(_) => None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::{HashMap, VecDeque};

    use serde_json::json;
    use tokio::sync::mpsc;

    use super::Daemon;
    use crate::config::Config;
    use crate::ha::registry::{AreaEntry, EntityEntry, Registry};

    fn entity(id: &str, area: &str) -> EntityEntry {
        EntityEntry {
            entity_id: id.into(),
            area_id: Some(area.into()),
            ..Default::default()
        }
    }

    fn daemon(entities: Vec<EntityEntry>, states: &[(&str, &str)]) -> Daemon {
        let (out, _rx) = mpsc::unbounded_channel();
        Daemon {
            config: Config::default(),
            config_path: std::path::PathBuf::from("/nonexistent/atrium-test.json"),
            generation: 0,
            registry: Registry {
                areas: vec![
                    AreaEntry {
                        area_id: "kitchen".into(),
                        name: "Kitchen".into(),
                        ..Default::default()
                    },
                    AreaEntry {
                        area_id: "hall".into(),
                        name: "Hall".into(),
                        ..Default::default()
                    },
                ],
                devices: Vec::new(),
                entities: entities.clone(),
            },
            entities: entities
                .into_iter()
                .map(|e| (e.entity_id.clone(), e))
                .collect(),
            connected_at: std::time::Instant::now(),
            registry_stale: false,
            states: states
                .iter()
                .map(|(id, state)| (id.to_string(), json!({ "entity_id": id, "state": state })))
                .collect::<HashMap<_, _>>(),
            out,
            history: RefCell::new(VecDeque::new()),
        }
    }

    #[test]
    fn a_room_switch_acts_only_on_what_the_room_shows() {
        let mut hidden = entity("light.hidden", "kitchen");
        hidden.hidden_by = Some("user".into());
        let mut config_light = entity("switch.config", "kitchen");
        config_light.entity_category = Some("config".into());
        let d = daemon(
            vec![
                entity("light.counter", "kitchen"),
                entity("light.island", "kitchen"),
                entity("switch.kettle", "kitchen"),
                entity("lock.back_door", "kitchen"),
                entity("cover.blind", "kitchen"),
                hidden,
                config_light,
                entity("light.hall", "hall"),
            ],
            &[
                ("light.counter", "on"),
                ("light.island", "off"),
                ("switch.kettle", "on"),
                ("lock.back_door", "unlocked"),
                ("cover.blind", "open"),
                ("light.hidden", "on"),
                ("switch.config", "on"),
                ("light.hall", "on"),
            ],
        );

        let (title, calls) = d.area_power_calls("area:kitchen", false).unwrap();
        assert_eq!(title, "Kitchen");
        let got: Vec<(&str, &str, Vec<&str>)> = calls
            .iter()
            .map(|c| {
                (
                    c.domain.as_str(),
                    c.service,
                    c.entity_ids.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                ("light", "turn_off", vec!["light.counter", "light.island"]),
                ("switch", "turn_off", vec!["switch.kettle"]),
            ]
        );
    }

    #[test]
    fn a_hidden_room_or_one_with_nothing_switchable_is_refused() {
        let mut d = daemon(
            vec![
                entity("lock.front", "hall"),
                entity("light.counter", "kitchen"),
            ],
            &[("lock.front", "locked"), ("light.counter", "off")],
        );
        assert!(d
            .area_power_calls("area:hall", true)
            .unwrap_err()
            .contains("nothing to switch on"));
        assert!(d.area_power_calls("area:attic", true).is_err());

        d.config.areas.hidden = vec!["kitchen".into()];
        assert!(
            d.area_power_calls("area:kitchen", true).is_err(),
            "a hidden room is not acted on"
        );
    }
}
