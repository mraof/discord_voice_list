#[macro_use]
extern crate serde_derive;
extern crate serde_json;

extern crate discord;
extern crate retry;

use discord::{Discord, State, Error, ChannelRef};
use discord::model::{Event, UserId};
use retry::Retry;
use std::fs::File;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

fn main() {
    let config = DiscordConfig::load("config.json");
    #[allow(deprecated)]
    let discord = Discord::new_cache("discord_tokens", &config.email, Some(&config.password))
        .expect("Login failed");

    let (mut connection, ready) =
        Retry::new(&mut || discord.connect(), &mut |result| result.is_ok())
            .wait_between(200, 60000)
            .execute()
            .unwrap()
            .unwrap();
    let mut state = State::new(ready);

    let mut voice_channel = None;
    let mut users = Users {
        filename: "names.txt".to_string(),
        ..Default::default()
    };
    let mut needs_update = true;

    loop {
        let event = match connection.recv_event() {
            Ok(event) => event,
            Err(err) => {
                println!("[Warning] Receive error: {:?}", err);
                if let Error::WebSocket(..) = err {
                    let (new_connection, ready) =
                        Retry::new(&mut || discord.connect(), &mut |result| result.is_ok())
                            .wait_between(200, 60000)
                            .execute()
                            .unwrap()
                            .unwrap();
                    connection = new_connection;
                    state = State::new(ready);
                }
                needs_update = true;
                continue;
            }
        };
        state.update(&event);
        if needs_update {
            match state.find_voice_user(state.user().id) {
                Some((_, channel_id)) => {
                    voice_channel = Some(channel_id);
                }
                None => {
                    voice_channel = None;
                }
            }
            users.clear();
            if let Some(voice_channel) = voice_channel {
                if let Some(ChannelRef::Public(server, _)) = state.find_channel(voice_channel) {
                    for member in &server.members {
                        let user = &member.user;
                        match state.find_voice_user(user.id) {
                            Some((_, channel_id)) if channel_id == voice_channel => {
                                users.insert(user.id, user.name.clone());
                            }
                            _ => (),
                        }
                    }
                }
            }
            println!("{:#?}", users);
            users.save();
            needs_update = false;
        }
        match event {
            Event::VoiceStateUpdate(server_id, voice_state) => {
                if voice_state.user_id == state.user().id {
                    voice_channel = voice_state.channel_id;
                    if let Some(server_id) = server_id {
                        connection.sync_servers(&[server_id]);
                    }
                }
                if voice_state.channel_id == voice_channel {
                    users.clear();
                    if let Some(voice_channel) = voice_channel {
                        if let Some(ChannelRef::Public(server, _)) =
                            state.find_channel(voice_channel)
                        {
                            for member in &server.members {
                                let user = &member.user;
                                match state.find_voice_user(user.id) {
                                    Some((_, channel_id)) if channel_id == voice_channel => {
                                        users.insert(user.id, user.name.clone());
                                    }
                                    _ => (),
                                }
                            }
                        }
                    }
                    println!("{:#?}", users);
                    users.save();
                } else {
                    if let Some(name) = users.remove(&voice_state.user_id) {
                        println!("{} left", name);
                    }
                }
            }
            _ => (),
        }
    }
}

#[derive(Default, Debug, Deserialize, Serialize)]
#[serde(default)]
struct DiscordConfig {
    email: String,
    password: String,
}

impl DiscordConfig {
    pub fn load(filename: &str) -> DiscordConfig {
        let config = if let Ok(file) = File::open(filename) {
            serde_json::from_reader(file).expect("Failed to parse discord config file")
        } else {
            DiscordConfig::default()
        };
        config.save(filename);
        config
    }

    pub fn save(&self, filename: &str) {
        serde_json::to_writer_pretty(
            &mut File::create(filename).expect("Could not create discord config file"),
            &self,
        ).expect("Failed to write discord config file");
    }
}

#[derive(Debug, Default)]
struct Users {
    users: HashMap<UserId, String>,
    filename: String,
}

impl Users {
    pub fn save(&self) {
        use std::io::Write;
        let contents = String::from("On call: ") +
            &self.users
                .values()
                .map(|name| name.to_string())
                .collect::<Vec<String>>()
                .join(", ");
        let mut file = File::create(&self.filename).expect("Unable to create file");
        file.write_all(contents.as_bytes()).expect(
            "Unable to write file",
        );
    }
}

impl Deref for Users {
    type Target = HashMap<UserId, String>;

    fn deref(&self) -> &Self::Target {
        &self.users
    }
}

impl DerefMut for Users {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.users
    }
}
