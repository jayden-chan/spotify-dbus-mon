use futures_util::StreamExt;
use zbus::{
    fdo::{DBusProxy, PropertiesProxy},
    names::{BusName, UniqueName},
    zvariant::Array,
    Connection, Result,
};

#[derive(Debug)]
enum PlaybackState {
    Playing,
    Paused,
}

static META_STR_MAX_LEN: usize = 100;
static SPOTIFY_BUS: &str = "org.mpris.MediaPlayer2.spotify";

#[tokio::main]
async fn main() -> Result<()> {
    let connection = Connection::session().await?;

    let props = PropertiesProxy::builder(&connection)
        .destination(SPOTIFY_BUS)?
        .path("/org/mpris/MediaPlayer2")?
        .build()
        .await?;

    let dbus_proxy = DBusProxy::new(&connection).await?;

    let mut props_changed = props.receive_properties_changed().await?;
    let mut owner_changed = dbus_proxy.receive_name_owner_changed().await?;

    let mut playback_state = PlaybackState::Paused;
    let mut meta = String::new();

    futures_util::try_join!(
        async {
            while let Some(signal) = props_changed.next().await {
                let args = signal.args()?;

                for (name, value) in args.changed_properties().iter() {
                    if *name == "PlaybackStatus" {
                        if let zbus::zvariant::Value::Str(status) = value {
                            match status.as_str() {
                                "Paused" => {
                                    println!("Paused");
                                    playback_state = PlaybackState::Paused;
                                }
                                "Playing" => {
                                    playback_state = PlaybackState::Playing;
                                    if !meta.is_empty() {
                                        println!("{meta}");
                                    }
                                }
                                _ => {
                                    eprintln!("unknown status {status}");
                                }
                            }
                        }
                    }

                    if *name == "Metadata" {
                        if let zbus::zvariant::Value::Dict(d) = value {
                            let mut title: Option<String> = None;
                            let mut artist: Option<String> = None;

                            if let Ok(Some(t)) = d.get::<&str, String>(&"xesam:title") {
                                title = Some(t);
                            } else {
                                eprintln!("no title");
                            }

                            if let Ok(Some(a)) = d.get::<&str, Array>(&"xesam:artist") {
                                if let Ok(a) = std::convert::TryInto::<Vec<String>>::try_into(a) {
                                    let artists_str = a.join(", ");
                                    artist = Some(artists_str);
                                }
                            } else {
                                eprintln!("no artist");
                            }

                            if let (Some(t), Some(a)) = (title, artist) {
                                let mut meta_str = format!("{a} - {t}");
                                if meta_str.len() > META_STR_MAX_LEN {
                                    meta_str = format!("{}...", &meta_str[0..META_STR_MAX_LEN]);
                                }
                                meta = meta_str;
                            }
                        }

                        if matches!(playback_state, PlaybackState::Playing) {
                            println!("{meta}");
                        }
                    }
                }
            }

            Ok::<(), zbus::Error>(())
        },
        async {
            while let Some(signal) = owner_changed.next().await {
                let args = signal.args()?;
                let name = args.name();
                let new_owner = args.new_owner();

                if let BusName::WellKnown(n) = name {
                    // spotify exited, print an empty line to hide the MPRIS widget
                    if *n == SPOTIFY_BUS {
                        let new_owner_inner: Option<UniqueName> = new_owner.clone().into();
                        if new_owner_inner.is_none() {
                            println!();
                        }
                    }
                }
            }

            Ok(())
        }
    )?;

    Ok(())
}
