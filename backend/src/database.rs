use std::{
    env,
    sync::{LazyLock, Mutex},
};

use anyhow::{Result, bail};
use rusqlite::{Connection, Params, Statement, types::Null};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::broadcast::{Receiver, Sender, channel};

use crate::{
    models::{Character, Identifiable, Localization, Map, Seeds, Settings},
    services::Event,
};

const MAPS: &str = "maps";
const CHARACTERS: &str = "characters";
const SETTINGS: &str = "settings";
const SEEDS: &str = "seeds";
const LOCALIZATIONS: &str = "localizations";

static CONNECTION: LazyLock<Mutex<Connection>> = LazyLock::new(|| {
    let path = env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("local.db");
    let conn = Connection::open(path.to_str().unwrap()).expect("failed to open local.db");
    conn.execute_batch(
        format!(
            r#"
            CREATE TABLE IF NOT EXISTS {MAPS} (
                id INTEGER PRIMARY KEY,
                data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS {CHARACTERS} (
                id INTEGER PRIMARY KEY,
                data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS {SETTINGS} (
                id INTEGER PRIMARY KEY,
                data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS {SEEDS} (
                id INTEGER PRIMARY KEY,
                data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS {LOCALIZATIONS} (
                id INTEGER PRIMARY KEY,
                data TEXT NOT NULL
            );
            "#
        )
        .as_str(),
    )
    .unwrap();
    Mutex::new(conn)
});
static EVENT: LazyLock<Sender<DatabaseEvent>> = LazyLock::new(|| channel(5).0);

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum DatabaseEvent {
    MapUpdated(Map),
    MapDeleted(i64),
    SettingsUpdated(Settings),
    LocalizationUpdated(Localization),
    CharacterUpdated(Character),
    CharacterDeleted(i64),
}

impl Event for DatabaseEvent {}

pub fn database_event_receiver() -> Receiver<DatabaseEvent> {
    EVENT.subscribe()
}

pub fn query_and_upsert_seeds() -> Seeds {
    let mut seeds = query_from_table::<Seeds>(SEEDS)
        .unwrap()
        .into_iter()
        .next()
        .unwrap_or_default();
    upsert_to_table(SEEDS, &mut seeds).unwrap();
    seeds
}

pub fn query_or_upsert_localization() -> Localization {
    let mut localization = query_from_table::<Localization>(LOCALIZATIONS)
        .unwrap()
        .into_iter()
        .next()
        .unwrap_or_default();
    if localization.id.is_none() {
        upsert_to_table(LOCALIZATIONS, &mut localization).unwrap();
    }
    localization
}

pub fn upsert_localization(localization: &mut Localization) -> Result<()> {
    upsert_to_table(LOCALIZATIONS, localization).inspect(|_| {
        let _ = EVENT.send(DatabaseEvent::LocalizationUpdated(localization.clone()));
    })
}

pub fn query_settings() -> Settings {
    let mut settings = query_from_table::<Settings>(SETTINGS)
        .unwrap()
        .into_iter()
        .next()
        .unwrap_or_default();
    if settings.id.is_none() {
        upsert_settings(&mut settings).unwrap();
    }
    settings
}

pub fn upsert_settings(settings: &mut Settings) -> Result<()> {
    upsert_to_table(SETTINGS, settings).inspect(|_| {
        let _ = EVENT.send(DatabaseEvent::SettingsUpdated(settings.clone()));
    })
}

pub fn query_characters() -> Result<Vec<Character>> {
    #[cfg(not(debug_assertions))]
    {
        query_from_table(CHARACTERS)
    }

    #[cfg(debug_assertions)]
    {
        let mut characters = query_from_table(CHARACTERS)?;
        if !characters.is_empty() {
            return Ok(characters);
        }

        let mut character = Character {
            name: "Test".to_string(),
            ..Default::default()
        };

        upsert_character(&mut character).unwrap();
        characters.push(character);

        Ok(characters)
    }
}

pub fn upsert_character(character: &mut Character) -> Result<()> {
    upsert_to_table(CHARACTERS, character).inspect(|_| {
        let _ = EVENT.send(DatabaseEvent::CharacterUpdated(character.clone()));
    })
}

pub fn delete_character(character: &Character) -> Result<()> {
    delete_from_table(CHARACTERS, character).inspect(|_| {
        let _ = EVENT.send(DatabaseEvent::CharacterDeleted(
            character.id.expect("valid id if deleted"),
        ));
    })
}

pub fn query_maps() -> Result<Vec<Map>> {
    #[cfg(not(debug_assertions))]
    {
        query_from_table(MAPS)
    }

    #[cfg(debug_assertions)]
    {
        use std::collections::HashMap;

        let mut maps = query_from_table(MAPS)?;
        if !maps.is_empty() {
            return Ok(maps);
        }

        let actions = HashMap::from([("Test".to_string(), vec![])]);
        let mut map = Map {
            name: "Test".to_string(),
            actions,
            ..Default::default()
        };

        upsert_map(&mut map).unwrap();
        maps.push(map);

        Ok(maps)
    }
}

pub fn upsert_map(map: &mut Map) -> Result<()> {
    upsert_to_table(MAPS, map).inspect(|_| {
        let _ = EVENT.send(DatabaseEvent::MapUpdated(map.clone()));
    })
}

pub fn delete_map(map: &Map) -> Result<()> {
    delete_from_table(MAPS, map).inspect(|_| {
        let _ = EVENT.send(DatabaseEvent::MapDeleted(
            map.id.expect("valid id if deleted"),
        ));
    })
}

fn map_data<T>(mut stmt: Statement<'_>, params: impl Params) -> Result<Vec<T>>
where
    T: DeserializeOwned + Identifiable + Default,
{
    Ok(stmt
        .query_map::<T, _, _>(params, |row| {
            let id = row.get::<_, i64>(0).unwrap();
            let data = row.get::<_, String>(1).unwrap();
            let mut value = serde_json::from_str::<'_, T>(data.as_str()).unwrap_or_default();
            value.set_id(id);
            Ok(value)
        })?
        .filter_map(|c| c.ok())
        .collect::<Vec<_>>())
}

fn query_from_table<T>(table: &str) -> Result<Vec<T>>
where
    T: DeserializeOwned + Identifiable + Default,
{
    let conn = CONNECTION.lock().unwrap();
    let stmt = format!("SELECT id, data FROM {table};");
    let stmt = conn.prepare(&stmt).unwrap();
    map_data(stmt, [])
}

fn upsert_to_table<T>(table: &str, data: &mut T) -> Result<()>
where
    T: Serialize + Identifiable,
{
    let json = serde_json::to_string(&data).unwrap();
    let conn = CONNECTION.lock().unwrap();
    let stmt = format!(
        "INSERT INTO {table} (id, data) VALUES (?1, ?2) ON CONFLICT (id) DO UPDATE SET data = ?2;",
    );
    match data.id() {
        Some(id) => {
            if conn.execute(&stmt, (id, &json))? > 0 {
                Ok(())
            } else {
                bail!("no row was updated")
            }
        }
        None => {
            if conn.execute(&stmt, (Null, &json))? > 0 {
                data.set_id(conn.last_insert_rowid());
                Ok(())
            } else {
                bail!("no row was inserted")
            }
        }
    }
}

fn delete_from_table<T: Identifiable>(table: &str, data: &T) -> Result<()> {
    fn inner(table: &str, id: Option<i64>) -> Result<()> {
        if let Some(id) = id {
            let conn = CONNECTION.lock().unwrap();
            let stmt = format!("DELETE FROM {table} WHERE id = ?1;");
            let deleted = conn.execute(&stmt, [id])?;

            if deleted > 0 {
                return Ok(());
            }
        }
        bail!("no row was deleted")
    }

    inner(table, data.id())
}
