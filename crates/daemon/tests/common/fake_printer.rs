//! A printer behind the fake Moonraker: Klipper's state, the objects `printer.objects.query`
//! can return, and the bridge token. Tests change it while the daemon runs.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};

use super::fake_moonraker::{FakeMoonraker, Script};

pub fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/data/moonraker/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

struct State {
    klippy_state: String,
    message: String,
    app: Option<String>,
    objects: Map<String, Value>,
    login_required: bool,
    /// `config.authorization` in the `server.config` answer; `None` leaves it out.
    authorization: Option<Value>,
    printer_info_fails: bool,
}

/// The printer's current state. Clones share it.
#[derive(Clone)]
pub struct FakePrinter {
    state: Arc<Mutex<State>>,
}

impl FakePrinter {
    /// Installs the bootstrap handlers on `fake`: identify, server and printer info, file roots,
    /// object queries and the bridge token (`"TOKEN"`). `config` is `configfile.config`; the
    /// toolhead is idle, homed and at the origin, and nothing is printing.
    pub fn new(fake: &FakeMoonraker, klippy_state: &str, config: Value) -> Self {
        let mut objects = Map::new();
        objects.insert(
            "configfile".to_owned(),
            json!({"config": config, "settings": {}}),
        );
        objects.insert(
            "toolhead".to_owned(),
            json!({
                "homed_axes": "xyz",
                "position": [0.0, 0.0, 0.0, 0.0],
                "print_time": 100.0,
                "estimated_print_time": 100.0,
                "axis_minimum": [0.0, 0.0, -2.0, 0.0],
                "axis_maximum": [301.0, 308.0, 300.0, 0.0],
            }),
        );
        objects.insert(
            "mcu".to_owned(),
            json!({"mcu_version": "v0.13.0-658-gbd09e017"}),
        );
        objects.insert("print_stats".to_owned(), json!({"state": "standby"}));
        objects.insert("idle_timeout".to_owned(), json!({"state": "Ready"}));
        let printer = Self {
            state: Arc::new(Mutex::new(State {
                klippy_state: klippy_state.to_owned(),
                message: "Printer is ready".to_owned(),
                app: None,
                objects,
                login_required: false,
                authorization: Some(
                    fixture("real_server_config.json")["config"]["authorization"].clone(),
                ),
                printer_info_fails: false,
            })),
        };
        fake.on(
            "server.connection.identify",
            Script::Result(json!({"connection_id": 1})),
        );
        fake.on("access.oneshot_token", Script::Result(json!("TOKEN")));
        let p = printer.clone();
        fake.on_fn("server.info", move |_, _| {
            let mut info = fixture("server_info_ready.json");
            info["klippy_state"] = json!(p.state.lock().unwrap().klippy_state);
            Script::Result(info)
        });
        let p = printer.clone();
        fake.on_fn("printer.info", move |_, _| {
            let s = p.state.lock().unwrap();
            if s.printer_info_fails {
                return Script::Error(-32601, "Klippy Host not connected");
            }
            let mut info = fixture("real_printer_info.json");
            info["state"] = json!(s.klippy_state);
            info["state_message"] = json!(s.message);
            if let Some(app) = &s.app {
                info["app"] = json!(app);
            }
            Script::Result(info)
        });
        let p = printer.clone();
        fake.on_fn("printer.objects.query", move |params, _| {
            Script::Result(p.query(params))
        });
        fake.on(
            "server.files.roots",
            Script::Result(fixture("files_roots.json")),
        );
        let p = printer.clone();
        fake.on_fn("server.config", move |_, _| {
            let s = p.state.lock().unwrap();
            let mut answer = fixture("real_server_config.json");
            match &s.authorization {
                Some(auth) => answer["config"]["authorization"] = auth.clone(),
                None => {
                    answer["config"]
                        .as_object_mut()
                        .unwrap()
                        .remove("authorization");
                }
            }
            Script::Result(answer)
        });
        let p = printer.clone();
        fake.on_fn("access.info", move |_, _| {
            let mut info = fixture("real_access_info.json");
            info["login_required"] = json!(p.state.lock().unwrap().login_required);
            Script::Result(info)
        });
        printer
    }

    /// Sets what `access.info` says about logins.
    pub fn set_login_required(&self, required: bool) {
        self.state.lock().unwrap().login_required = required;
    }

    /// Replaces `config.authorization` in the `server.config` answer, or removes it.
    pub fn set_authorization(&self, authorization: Option<Value>) {
        self.state.lock().unwrap().authorization = authorization;
    }

    /// Makes `printer.info` fail, as it does while Klippy is disconnected.
    pub fn set_printer_info_fails(&self, fails: bool) {
        self.state.lock().unwrap().printer_info_fails = fails;
    }

    fn query(&self, params: &Value) -> Value {
        let s = self.state.lock().unwrap();
        let mut status = Map::new();
        for (name, fields) in params["objects"].as_object().into_iter().flatten() {
            let Some(object) = s.objects.get(name) else {
                continue;
            };
            let kept = match fields.as_array() {
                Some(fields) => {
                    let mut out = Map::new();
                    for f in fields.iter().filter_map(Value::as_str) {
                        if let Some(v) = object.get(f) {
                            out.insert(f.to_owned(), v.clone());
                        }
                    }
                    Value::Object(out)
                }
                None => object.clone(),
            };
            status.insert(name.clone(), kept);
        }
        json!({"eventtime": 1.0, "status": status})
    }

    pub fn set_state(&self, state: &str) {
        self.state.lock().unwrap().klippy_state = state.to_owned();
    }

    pub fn set_message(&self, message: &str) {
        self.state.lock().unwrap().message = message.to_owned();
    }

    pub fn set_app(&self, app: Option<&str>) {
        self.state.lock().unwrap().app = app.map(str::to_owned);
    }

    pub fn set_config(&self, config: Value) {
        self.set_field("configfile", "config", config);
    }

    /// Sets `configfile.settings`.
    pub fn set_settings(&self, settings: Value) {
        self.set_field("configfile", "settings", settings);
    }

    /// Replaces the field `field` of the object `name`.
    pub fn set_field(&self, name: &str, field: &str, value: Value) {
        let mut s = self.state.lock().unwrap();
        let object = s
            .objects
            .entry(name.to_owned())
            .or_insert_with(|| json!({}));
        object[field] = value;
    }

    /// Removes the object `name`, so a query for it answers without it.
    pub fn remove_object(&self, name: &str) {
        self.state.lock().unwrap().objects.remove(name);
    }

    /// The current value of the field `field` of the object `name`.
    pub fn field(&self, name: &str, field: &str) -> Value {
        self.state.lock().unwrap().objects[name][field].clone()
    }
}
