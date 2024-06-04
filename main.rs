#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;

use regex::Regex;
use mysql::prelude::*;
use mysql::Pool;
use std::thread;
use reqwest::ResponseBuilderExt;
use std::collections::HashMap;
use rocket_contrib::json::Json;
use urlencoding::encode;
use std::time::{Duration, SystemTime};
use chrono::prelude::*;
use chrono::{Local, NaiveDateTime};
use rocket_contrib::json::JsonValue;
use std::net::TcpListener;
use std::thread::spawn;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sqlite::Connection;
use serde_json::json;
use mysql::*;
use std::str::FromStr;
use rand::Rng;
use sqlite::State;
use rocket::response::content;
use totp_rs::{Algorithm, Secret, TOTP};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_LENGTH, CONTENT_TYPE};
use serde_json::{Map, Value};
static mut CSRF: Option<String> = None;
use hmac::{Hmac, Mac, NewMac};
use sha2::Sha256;
use std::convert::TryInto;
use rand::rngs::OsRng;
use std::sync::{mpsc, Arc, Mutex};
use tungstenite::{accept, Message};
const CLEANUP_INTERVAL_SECONDS: u64 = 60;


#[derive(Debug, Deserialize, Serialize)]
struct UserInfo {
    UserID: u64,
    UserName: String,
    RobuxBalance: u64,
    ThumbnailUrl: String,
    IsAnyBuildersClubMember: bool,
    IsPremium: bool,
}
#[derive(Debug, Deserialize)]
struct HeadShot {
    id: u64,
    name: String,
    url: String,
    thumbnailFinal: bool,
    thumbnailUrl: String,
    bcOverlayUrl: Option<String>,
    substitutionType: u32,
}

#[derive(Deserialize)]
struct FlipResponse {
    server_seed: String,
    client_seed: String,
    result: f64,
}
struct AssetInfo {
    AssetId: String,
}

#[derive(Debug, Serialize)]
struct User {
    id: u64,
    userid: u64,
    authkey: String,
    rbxprofit: u64,
    btcprofit: u64,
    usertrust: String,
    btcbet: u64,
    rbxbet: u64,
    gamesplayed: u64,
    gameswon: u64,
    gameslost: u64,
    totalitems: u64
}
#[derive(Serialize)]
struct BotData {
    cookie: String,
    password: String,
    otpkey: String,
}
#[derive(Serialize, Deserialize)]
struct CoinflipResult {
    server_seed: String,
    client_seed: String,
    result: f64,
}

#[derive(Serialize, Deserialize)]
struct Game {
    id: u64,
    gameid: Option<u64>,
    totalrap: Option<u64>,
    uuaids: Option<serde_json::Value>,
    gamemode: Option<String>,
    tixbux: Option<u64>,
    isrbxgame: Option<bool>,
    status: Option<u64>,
    winner: Option<u64>,
    assetids: Option<serde_json::Value>,
    headshot1: Option<String>,
    headshot2: Option<String>,
    
}

#[derive(Serialize, Deserialize)]
struct Gamecore {
    id: u64,
    gameid: Option<u64>,
    player1:  Option<u64>,  
    player2: Option<u64>
    
}

#[derive(Debug, Serialize)]
enum ApiError {
    #[serde(rename = "status")]
    Status(u32),
}

#[derive(Serialize)]
struct Server {
    status: u16,
    message: String,
}

#[derive(Debug, Deserialize)]

struct AssetIDS {
    AssetId: u64,
    Onhold: bool,
    itemprice: u64,
}

#[derive(Debug, Deserialize)]
struct Userreal {
    UserAssetIds: Vec<u64>,
    UserAssetInfo: HashMap<String, AssetIDS>,
}

#[derive(Debug, Deserialize)]
struct UserInfoTest {
    UserAssetInfo: Option<HashMap<u64, UserAssetReal>>,
}

#[derive(Debug, Deserialize)]
struct UserAssetReal {
    itemprice: Option<u64>,
}




type PeerMap = Arc<Mutex<HashMap<u64, tungstenite::WebSocket<std::net::TcpStream>>>>;

pub type Broadcaster = dyn Fn(String) + Send;

static mut BROADCASTER: Option<Box<Broadcaster>> = None;

pub fn create_broadcaster() -> Box<Broadcaster> {
    let server = TcpListener::bind("127.0.0.1:8080").unwrap();
    let peers: PeerMap = Arc::new(Mutex::new(HashMap::new()));

    println!("collecting peers");
    // collect peer list on another thread
    let peers1 = peers.clone();
    spawn(move || {
        for (client_id, stream) in server.incoming().enumerate() {
            let websocket = accept(stream.unwrap()).unwrap();
            println!("Registered client {}", client_id);
            peers1.lock().unwrap().insert(client_id as u64, websocket);
        }
    });

    println!("creating closure");
    return Box::new(move |msg| {
        let mut disconnected_clients = vec![];
        for (client_id, sock) in peers.lock().unwrap().iter_mut() {
            let msg1 = msg.clone();
            let out = Message::from(msg1);
            if let Err(_) = sock.write_message(out) {
                // Error while writing to WebSocket, client likely disconnected
                disconnected_clients.push(*client_id);
            }
        }
        // Remove disconnected clients
        let mut peers = peers.lock().unwrap();
        for client_id in disconnected_clients {
            peers.remove(&client_id);
        }
    });
}

fn set_broadcaster(broadcaster: Box<Broadcaster>) {
    unsafe {
        BROADCASTER = Some(broadcaster);
    }
}

fn get_user_item_ids(uuaids: &str, player_cookie: &str) -> String{
    let player_cookie = player_cookie.trim();
    let userinfo_response = reqwest::blocking::get(format!("http://localhost:8000/userinfo/{}", player_cookie))
        .unwrap_or_else(|e| {
            panic!("Failed to get userinfo for player_cookie: {}. Error: {:?}", player_cookie, e);
        });
    
    let userinfo_data: Userreal = userinfo_response.json().unwrap_or_else(|e| {
        panic!("Failed to parse userinfo JSON. Error: {:?}", e);
    });

    let uuaids: Vec<u64> = uuaids.split(',').map(|id_str| {
        id_str.trim().trim_matches(|c| c == '[' || c == ']').parse::<u64>().unwrap_or_else(|e| {
            panic!("Failed to parse UUAID {} into u64. Error: {:?}", id_str, e);
        })
    }).collect();

    let mut asset_ids = Vec::new();

    for uua_id in uuaids {
        if userinfo_data.UserAssetIds.contains(&(uua_id as u64)) {
            if let Some(asset_info) = userinfo_data.UserAssetInfo.get(&uua_id.to_string()) {
                asset_ids.push(asset_info.AssetId);
            }
        } else {
            println!("UUAID {} not found in user's assets.", uua_id);
        }
    }
    let asset_ids_json = serde_json::to_string(&asset_ids).unwrap();
    println!("{}", asset_ids_json);
    asset_ids_json
}




// Define the function to get the user's rap sum
fn get_user_rap_sum(uuaids: &str, player_cookie: &str) -> u64 {
    let encoded_rbx_cookie = if !player_cookie.is_empty() {
        Some(player_cookie)
    } else {
        None
    };
    println!("{}",uuaids);
    if encoded_rbx_cookie.is_none() {
        println!("Cookie Could Not be pulled");
        return 0;
    }

    let client = Client::new();

    let userinfo_response = client
        .get(&format!("http://localhost:8000/userinfo/{}", encoded_rbx_cookie.unwrap()))
        .send()
        .unwrap();

    if !userinfo_response.status().is_success() {
        panic!("Userinfo API returned status {}", userinfo_response.status());
    }

    let userinfo_data: UserInfoTest = userinfo_response.json().unwrap();

    println!("{:?}", userinfo_data);

    // Check if UserAssetInfo exists and is an object
    let user_asset_info: HashMap<u64, UserAssetReal> = match userinfo_data.UserAssetInfo {
        Some(info) => info,
        None => {
            println!("Invalid UserAssetInfo");
            return 0;
        }
    };
    // Parse the uuaids string and extract the individual ids
    let re = Regex::new(r"\d+").unwrap();
    let uuaids_vec: Vec<u64> = re
        .find_iter(uuaids)
        .filter_map(|id| id.as_str().parse().ok())
        .collect();
    // Calculate the rap sum for the provided uuaids from UserAssetInfo in userinfoData
    let mut rap_sum = 0;
    for uua_id in uuaids_vec {
        if let Some(uua_info) = user_asset_info.get(&uua_id) {
            if let Some(item_price) = uua_info.itemprice {
                rap_sum += item_price;
            }
        }
    }

    rap_sum
}

#[get("/games")]
fn get_games(pool: rocket::State<Pool>) -> Json<HashMap<u64, Game>> {
    let mut conn = pool.get_conn().expect("Failed to get database connection");

    let query = "SELECT id, gameid, uuaids, totalrap, gamemode, tixbux, isrbxgame, status, winner, assetids, headshot1, headshot2 FROM flipusers.games";

    let games: Vec<Game> = conn
        .exec_map(query, (), |(id, gameid, uuaids, totalrap, gamemode, tixbux, isrbxgame, status, winner, assetids, headshot1, headshot2)| Game {
            id,
            gameid,
            uuaids,
            totalrap,
            gamemode,
            tixbux,
            isrbxgame,
            status,
            winner,
            assetids,
            headshot1,
            headshot2,
        })
        .expect("Failed to execute query");

    let game_map: HashMap<u64, Game> = games.into_iter().map(|game| (game.gameid.unwrap_or(0), game)).collect();
    Json(game_map)
}
#[get("/gamecore")]
fn core_games(pool: rocket::State<Pool>) -> Json<HashMap<u64, Gamecore>> {
    let mut conn = pool.get_conn().expect("Failed to get database connection");

    let query = "SELECT id, gameid, player1, player2 FROM flipusers.games";

    let games: Vec<Gamecore> = conn
        .exec_map(query, (), |(id, gameid, player1, player2)| Gamecore {
            id,
            gameid,
            player1,
            player2
        })
        .expect("Failed to execute query");

    let game_map: HashMap<u64, Gamecore> = games.into_iter().map(|game| (game.gameid.unwrap_or(0), game)).collect();
    Json(game_map)
}


#[post("/join_game", data = "<join_data>")]
fn join_game(pool: rocket::State<Pool>, join_data: Json<JsonValue>) -> Json<JsonValue> {
    let game_id = join_data["gameid"].as_u64(); // Assuming gameid is provided in the JSON data
    if let Some(game_id) = game_id {
        let player2 = join_data["player2"].as_u64();
        let new_uuaids = join_data["uuaids"].clone();
        let playercookie2: Option<&str> = join_data["playercookie2"].as_str();
        let cleancookie: String = match playercookie2 {
            Some(cookie) => url_encode(cookie),
            None => String::new(), // Handle the case when playercookie2 is None based on your use case
        };
        let personalpfp: String = fetch_headshot_url(&cleancookie);
        let finalids = get_user_item_ids(&new_uuaids.to_string(),&cleancookie);
        let tatalrrap = get_user_rap_sum(&new_uuaids.to_string(),&cleancookie);
        let personalpfp = fetch_headshot_url(&cleancookie);
        let mut conn = pool.get_conn().expect("Failed to get database connection");

        let query = "SELECT uuaids FROM flipusers.games WHERE gameid = :gameid";
        let params: Params = params! {
            "gameid" => game_id,
        };

        let uuaids_result: Option<Vec<u8>> = conn
            .exec_first(query, params)
            .expect("Failed to execute query");
        let uuaids: Option<Vec<u64>> = uuaids_result
            .map(|uuaids_bytes| {
                serde_json::from_slice(&uuaids_bytes).expect("Failed to deserialize uuaids")
            })
            .flatten();
        let updated_uuaids = if let Some(mut existing_uuaids) = uuaids {
            if let Some(new_uuaids) = new_uuaids.as_array() {
                for uuaid in new_uuaids {
                    if let Some(uuaid_value) = uuaid.as_u64() {
                        existing_uuaids.push(uuaid_value);
                    }
                }
            }
            existing_uuaids
        } else {
            new_uuaids
                .as_array()
                .map(|arr| arr.iter().filter_map(|uuaid| uuaid.as_u64()).collect())
                .unwrap_or_else(Vec::new)
        };
        let uuaids_bytes = serde_json::to_vec(&updated_uuaids).expect("Failed to serialize uuaids");
        let assetids_query = "SELECT assetids FROM flipusers.games WHERE gameid = :gameid";
        let assetids_params: Params = params! {
            "gameid" => game_id,
        };

        let assetids_result: Option<Vec<u8>> = conn
            .exec_first(assetids_query, assetids_params.clone())
            .expect("Failed to execute assetids query");
        let assetids: Option<Vec<u64>> = assetids_result
            .map(|assetids_bytes| {
                serde_json::from_slice(&assetids_bytes).expect("Failed to deserialize assetids")
            })
            .flatten();

        // Deserialize the finalids String into a Vec<u64>
        let finalids: Vec<u64> = serde_json::from_str(&finalids)
            .expect("Failed to deserialize finalids into Vec<u64>");

        let updated_assetids = if let Some(mut existing_assetids) = assetids {
            // Merge the existing_assetids with finalids
            existing_assetids.extend(finalids);
            existing_assetids
        } else {
            finalids
        };
        let assetids_bytes =
            serde_json::to_vec(&updated_assetids).expect("Failed to serialize assetids");

            let current_rapvalue: u64 = {
                let query = "SELECT totalrap FROM flipusers.games WHERE gameid = :gameid";
                let params: Params = params! {
                    "gameid" => game_id,
                };
            
                let rapvalue_result: Option<u64> = conn
                    .exec_first(query, params)
                    .expect("Failed to execute rapvalue query");
            
                rapvalue_result.unwrap_or(0)
            };
            
            let updated_rapvalue = current_rapvalue + tatalrrap;
            let update_query = "UPDATE flipusers.games SET player2 = :player2, status = :status, uuaids = :uuaids, playercookie2 = :playercookie2, headshot2 = :headshot2, assetids = :assetids, totalrap = :totalrap WHERE gameid = :gameid";
            let update_params: Params = params! {
                "gameid" => game_id,
                "player2" => player2,
                "status" => 2,
                "uuaids" => uuaids_bytes,
                "playercookie2" => playercookie2,
                "headshot2" => personalpfp,
                "assetids" => assetids_bytes,
                "totalrap" => updated_rapvalue // Use the updated rapvalue here
            };
                        
            conn.exec_drop(update_query, update_params).expect("Failed to execute update query");

        let response = json!({
            "Status": 200,
            "Message": format!("Player joined game with gameid {}", game_id)
        });
        if let Some(broadcaster) = unsafe { BROADCASTER.as_ref() } {
            broadcaster("joingame".to_string());
        }
        let response = json!({
            "Status": 400,
            "Message": "Invalid or missing gameid in the request data"
        });
        let api_url = "http://localhost:8000/coinflip"; 
        let responsse = reqwest::blocking::get(api_url).unwrap().text().unwrap();
        let api_response: FlipResponse = serde_json::from_str(&responsse).unwrap();
        println!("Result: {}", api_response.result);
            if api_response.result >= 0.5 {
            wingame(game_id,1)
        } else {
            wingame(game_id,2)
        }
        Json(rocket_contrib::json::JsonValue(response))
    } else {
        let response = json!({
            "Status": 400,
            "Message": "Invalid or missing gameid in the request data"
        });
        Json(rocket_contrib::json::JsonValue(response))
    }
}





//THIS IS ONLY FOR PVP GAMES

#[post("/pvp_join_game", data = "<join_data>")]
fn pvp_join_game(pool: rocket::State<Pool>, join_data: Json<JsonValue>) -> Json<JsonValue> {
    let game_id = join_data["gameid"].as_u64(); // Assuming gameid is provided in the JSON data
    if let Some(game_id) = game_id {
        let player2 = join_data["player2"].as_u64();
        let new_uuaids = join_data["uuaids"].clone();
        let playercookie2: Option<&str> = join_data["playercookie2"].as_str();
        let cleancookie: String = match playercookie2 {
            Some(cookie) => url_encode(cookie),
            None => String::new(), // Handle the case when playercookie2 is None based on your use case
        };
        let personalpfp: String = fetch_headshot_url(&cleancookie);
        let finalids = get_user_item_ids(&new_uuaids.to_string(),&cleancookie);
        let tatalrrap = get_user_rap_sum(&new_uuaids.to_string(),&cleancookie);
        let personalpfp = fetch_headshot_url(&cleancookie);
        let mut conn = pool.get_conn().expect("Failed to get database connection");

        let query = "SELECT uuaids FROM flipusers.games WHERE gameid = :gameid";
        let params: Params = params! {
            "gameid" => game_id,
        };

        let uuaids_result: Option<Vec<u8>> = conn
            .exec_first(query, params)
            .expect("Failed to execute query");
        let uuaids: Option<Vec<u64>> = uuaids_result
            .map(|uuaids_bytes| {
                serde_json::from_slice(&uuaids_bytes).expect("Failed to deserialize uuaids")
            })
            .flatten();
        let updated_uuaids = if let Some(mut existing_uuaids) = uuaids {
            if let Some(new_uuaids) = new_uuaids.as_array() {
                for uuaid in new_uuaids {
                    if let Some(uuaid_value) = uuaid.as_u64() {
                        existing_uuaids.push(uuaid_value);
                    }
                }
            }
            existing_uuaids
        } else {
            new_uuaids
                .as_array()
                .map(|arr| arr.iter().filter_map(|uuaid| uuaid.as_u64()).collect())
                .unwrap_or_else(Vec::new)
        };
        let uuaids_bytes = serde_json::to_vec(&updated_uuaids).expect("Failed to serialize uuaids");
        let assetids_query = "SELECT assetids FROM flipusers.games WHERE gameid = :gameid";
        let assetids_params: Params = params! {
            "gameid" => game_id,
        };

        let assetids_result: Option<Vec<u8>> = conn
            .exec_first(assetids_query, assetids_params.clone())
            .expect("Failed to execute assetids query");
        let assetids: Option<Vec<u64>> = assetids_result
            .map(|assetids_bytes| {
                serde_json::from_slice(&assetids_bytes).expect("Failed to deserialize assetids")
            })
            .flatten();

        // Deserialize the finalids String into a Vec<u64>
        let finalids: Vec<u64> = serde_json::from_str(&finalids)
            .expect("Failed to deserialize finalids into Vec<u64>");

        let updated_assetids: Vec<u64> = if let Some(mut existing_assetids) = assetids {
            // Merge the existing_assetids with finalids
            existing_assetids.extend(finalids);
            existing_assetids
        } else {
            finalids
        };
        let assetids_bytes =
            serde_json::to_vec(&updated_assetids).expect("Failed to serialize assetids");

            let current_rapvalue: u64 = {
                let query = "SELECT totalrap FROM flipusers.games WHERE gameid = :gameid";
                let params: Params = params! {
                    "gameid" => game_id,
                };
            
                let rapvalue_result: Option<u64> = conn
                    .exec_first(query, params)
                    .expect("Failed to execute rapvalue query");
            
                rapvalue_result.unwrap_or(0)
            };
            
            let updated_rapvalue = current_rapvalue + tatalrrap;
            let update_query = "UPDATE flipusers.games SET player2 = :player2, status = :status, uuaids = :uuaids, playercookie2 = :playercookie2, headshot2 = :headshot2, assetids = :assetids, totalrap = :totalrap WHERE gameid = :gameid";
            let update_params: Params = params! {
                "gameid" => game_id,
                "player2" => player2,
                "status" => 2,
                "uuaids" => uuaids_bytes,
                "playercookie2" => playercookie2,
                "headshot2" => personalpfp,
                "assetids" => assetids_bytes,
                "totalrap" => updated_rapvalue // Use the updated rapvalue here
            };
                        
            conn.exec_drop(update_query, update_params).expect("Failed to execute update query");

        let response = json!({
            "Status": 200,
            "Message": format!("Player joined game with gameid {}", game_id)
        });
        if let Some(broadcaster) = unsafe { BROADCASTER.as_ref() } {
            broadcaster("joingame".to_string());
        }
        let response = json!({
            "Status": 400,
            "Message": "Invalid or missing gameid in the request data"
        });
        Json(rocket_contrib::json::JsonValue(response))
    } else {
        let response = json!({
            "Status": 400,
            "Message": "Invalid or missing gameid in the request data"
        });
        Json(rocket_contrib::json::JsonValue(response))
    }
}




//END OF THIS IS ONLY FOR PVP GAMES












fn wingame(gameid: u64,result: u64){ //GG BOY KISSER
    let url: &str = "mysql://root:admin@localhost:3306/flipusers";
    let pool = Pool::new(url).expect("Failed to create database pool");
    let mut conn = pool.get_conn().expect("Failed to get database connection");
    let update_query = "UPDATE flipusers.games SET  winner = :winner, status = :status WHERE gameid = :gameid";
    let update_params: Params = params! {
        "gameid" => gameid,
        "winner" => result,
        "status" => 3
    };
    conn.exec_drop(update_query, update_params).expect("Failed to execute update query");
    let player1coin: u64 = {
        let query = "SELECT tixbux FROM flipusers.games WHERE gameid = :gameid";
        let params: Params = params! {
            "gameid" => gameid,
        };
    
        let rapvalue_result: Option<u64> = conn
            .exec_first(query, params)
            .expect("Failed to execute rapvalue query");
        rapvalue_result.unwrap_or(0)
    };
    let player1: u64 = {
        let query = "SELECT player1 FROM flipusers.games WHERE gameid = :gameid";
        let params: Params = params! {
            "gameid" => gameid,
        };
    
        let rapvalue_result: Option<u64> = conn
            .exec_first(query, params)
            .expect("Failed to execute rapvalue query");
        rapvalue_result.unwrap_or(0)
    };
    let player2: u64 = {
        let query = "SELECT player2 FROM flipusers.games WHERE gameid = :gameid";
        let params: Params = params! {
            "gameid" => gameid,
        };
    
        let rapvalue_result: Option<u64> = conn
            .exec_first(query, params)
            .expect("Failed to execute rapvalue query");
        rapvalue_result.unwrap_or(0)
    };
        let uuaids: Value = {
        let query = "SELECT uuaids FROM flipusers.games WHERE gameid = :gameid";
        let params: Params = params! {
            "gameid" => gameid,
        };
    
        let rapvalue_result: Option<String> = conn
            .exec_first(query, params)
            .expect("Failed to execute rapvalue query");
        serde_json::Value::String(rapvalue_result.unwrap_or("0".to_owned()))
    };
    tradelimiteds(player1, player2, result, uuaids.to_string())
}

fn tradelimiteds(player1: u64, player2: u64,winner: u64,uuaids: String){
    
}



#[get("/user/<user_id>")]
fn get_user(pool: rocket::State<Pool>, user_id: u64) -> Option<Json<serde_json::Value>> {
    let mut conn = pool.get_conn().expect("Failed to get database connection");

    let query = "SELECT id, userid, authkey, rbxprofit, btcprofit, usertrust, btcbet, rbxbet, gamesplayed, gameswon, gameslost,totalitems FROM users WHERE userid = :userid";
    let params: Params = params! {
        "userid" => user_id,
    };

    let result: Vec<User> = conn
        .exec_map(query, params, |(id, userid, authkey, rbxprofit, btcprofit, usertrust, btcbet, rbxbet, gamesplayed, gameswon, gameslost,totalitems): (u64, u64, Vec<u8>, u64, u64, Vec<u8>, u64, u64, u64, u64, u64,u64)| User {
            id,
            userid,
            authkey: String::from_utf8_lossy(&authkey).to_string(),
            rbxprofit,
            btcprofit,
            usertrust: String::from_utf8_lossy(&usertrust).to_string(),
            btcbet,
            rbxbet,
            gamesplayed,
            gameswon,
            gameslost,
            totalitems
        })
        .expect("Failed to execute query");

    if let Some(user) = result.first() {
        let success_message = json!({
            "Status": 200,
            "User_ID": user.userid,
            "Auth_Key": user.authkey,
            "RbxProfit": user.rbxprofit,
            "BtcProfit": user.btcprofit,
            "UserTrust": user.usertrust,
            "BtcBet": user.btcbet,
            "RbxBet": user.rbxbet,
            "GamesPlayed": user.gamesplayed,
            "GamesWon": user.gameswon,
            "GamesLost": user.gameslost,
            "TotalItems": user.totalitems
        });

        Some(Json(success_message.into()))
    } else {
        let error_message = json!({
            "Status": 400,
            "Message": "User Not Found"
        });

        Some(Json(error_message.into()))
    }
}


#[get("/updateauth/<user_id>/<auth_token>")]
fn update_user_auth_key(pool: rocket::State<Pool>, user_id: u64, auth_token: String) -> Option<Json<serde_json::Value>> {

    let mut conn = pool
        .get_conn()
        .expect("Failed to get database connection");

    let query = "UPDATE users SET authkey = :new_auth_key WHERE userid = :userid";
    let params: Params = params! {
        "new_auth_key" => auth_token,
        "userid" => user_id,
    };

    match conn.exec_map(query, params, |row: Row| row) {
        Ok(result) => {
            if result.len() == 1 {
                let success_message = json!({
                    "Status": 200,
                    "Message": "Auth Key Updated Successfully",
                });
                return Some(Json(success_message));
            } else {
                let error_message = json!({
                    "Status": 409,
                    "Message": "This is Fine Most Likely Not An Error",
                });
                return Some(Json(error_message));
            }
        }
        Err(err) => {
            eprintln!("Failed to execute query: {:?}", err);
            return Some(Json(json!({
                "Status": 500,
                "Message": "Internal Server Error",
            })));
        }
    }
}



#[get("/adduser/<user_id>/<auth_key>/<usertrust>")]
fn add_user(user_id: u64, auth_key: String, usertrust: String) -> Json<serde_json::Value> {
    let url = "mysql://root:admin@localhost/flipusers";
    let pool = mysql::Pool::new(url).unwrap();
    let now: NaiveDateTime = Local::now().naive_local();
    let expiration_date = now + chrono::Duration::minutes(10);
    let mut conn = pool.get_conn().unwrap();
    let select_query = "SELECT userid FROM users WHERE userid = :userid";
    let existing_user: Option<u64> = conn
        .exec_first(
            select_query,
            params! {
                "userid" => user_id,
            },
        )
        .unwrap();

    if existing_user.is_some() {
        let error_message = json!({
            "Status": 409,
            "Message": "User ID already exists in the database",
        });
        return Json(error_message);
    }

    let insert_query = "INSERT INTO users (userid, authkey,rbxprofit,btcprofit,usertrust,btcbet,rbxbet,gamesplayed,gameswon,gameslost,totalitems) VALUES (:userid, :authkey, :rbxprofit, :btcprofit, :usertrust, :btcbet, :rbxbet, :gamesplayed, :gameswon, :gameslost, :totalitems)";
    let _ = conn.exec_drop(
        insert_query,
        params! {
            "userid" => user_id,
            "authkey" => &auth_key,
            "rbxprofit" => 0,
            "btcprofit" => 0,
            "usertrust" => usertrust,
            "btcbet" => 0,
            "rbxbet" => 0,
            "gamesplayed" => 0,
            "gameswon" => 0,
            "gameslost" => 0,
            "totalitems" => 0
        },
    );

    let checksum_query = "SELECT LAST_INSERT_ID() AS checksum";
    let checksum: Option<u64> = conn.query_first(checksum_query).unwrap();

    match checksum {
        Some(checksum) => {
            let success_message = json!({
                "Status": 200,
                "Message": "User Added Successfully",
                "Checksum": checksum,
            });
            Json(success_message)
        }
        None => {
            let error_message = json!({
                "Status": 500,
                "Message": "Failed to retrieve checksum",
            });
            Json(error_message)
        }
    }
}
fn generate_unique_random_string() -> String {
    let mut rng = rand::thread_rng();
    let mut random_string = String::with_capacity(10);

    for _ in 0..10 {
        let digit = rng.gen_range(0..=9);
        random_string.push_str(&digit.to_string());
    }

    random_string
}
fn url_encode(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

fn fetch_headshot_url(player_cookie: &str) -> String {


    let url = format!("http://localhost:8000/userinfo/{}", player_cookie);

    println!("{}", url);

    let client = reqwest::blocking::Client::new();
    let headshot_response = client.get(&url).send().unwrap();

    if !headshot_response.status().is_success() {
        panic!(
            "Headshot API returned status {}",
            headshot_response.status()
        );
    }

    let headshot_data: HashMap<String, serde_json::Value> = headshot_response.json().unwrap();

    let headshot_url = headshot_data
        .get("HeadShot")
        .and_then(|url| url.as_str())
        .unwrap_or("https://cdn.discordapp.com/attachments/1100571341553414277/1133211858501902407/HeadshotBlackTransparent.png")
        .replace("60/60", "150/150");

    headshot_url.to_string()
}

#[get("/addgame/<player1>/<gamemode>/<tixbux>/<isrbxgame>/<uuaids>/<playercookie1>")]
fn add_game(player1: u64, gamemode: String, tixbux: u64, isrbxgame: bool, uuaids: String, playercookie1: String) -> Json<serde_json::Value> {
    let gameid = generate_unique_random_string();
    let url = "mysql://root:admin@localhost/flipusers";
    let pool: Pool = mysql::Pool::new(url).unwrap();
    let cleancookie: String = url_encode(&playercookie1);
    println!("clean cookies: {}",cleancookie);
    let finalids = get_user_item_ids(&uuaids,&cleancookie);
    let tatalrrap = get_user_rap_sum(&uuaids,&cleancookie);
    let personalpfp = fetch_headshot_url(&cleancookie);
    println!("tatalrrap: {}",tatalrrap);
    let now = Local::now().naive_local();
    let expiration_date = now + chrono::Duration::minutes(10);
    let expiration_date_str = expiration_date.format("%Y-%m-%d %H:%M:%S").to_string(); // Convert to string representation
    let mut conn = pool.get_conn().unwrap();
    let insert_query = "INSERT INTO games (gameid,player1,player2,gamemode,tixbux,isrbxgame,status,winner,uuaids,playercookie1,playercookie2,expiration_date,assetids,totalrap,headshot1,headshot2) VALUES (:gameid, :player1, :player2, :gamemode, :tixbux, :isrbxgame, :status, :winner, :uuaids, :playercookie1, :playercookie2, :expiration_date, :assetids, :totalrap, :headshot1,:headshot2)";
    let insert_result = conn.exec_drop(
        insert_query,
        params! {
            "gameid" => gameid.clone(), // Clone the gameid to avoid moving it
            "player1" => player1,
            "player2" => 0,
            "gamemode" => &gamemode,
            "tixbux" => tixbux,
            "isrbxgame" => isrbxgame,
            "status" => 1, // 1 means the game is open, 2 in progress, and 3 done
            "winner" => 0,
            "uuaids" => &uuaids, // 0 aka no winner yet
            "playercookie1" => playercookie1,
            "playercookie2" => 0,
            "expiration_date" => expiration_date_str, // Use the string representation
            "assetids" => finalids,
            "totalrap" => tatalrrap,
            "headshot1" => &personalpfp,
            "headshot2" => "https://cdn.discordapp.com/attachments/1100571341553414277/1133211858501902407/HeadshotBlackTransparent.png"
        },
    );
    match insert_result {
        Ok(()) => {
            println!("SQL Response - Insert Successful. Affected Rows: 1");
            if let Some(broadcaster) = unsafe { BROADCASTER.as_ref() } {
                broadcaster("newgame".to_string());
            }

        }
        Err(err) => {
            println!("SQL Response - Insert Failed. Error: {}", err);
        }
    }
    let checksum_query = "SELECT LAST_INSERT_ID() AS checksum";
    let checksum: Option<u64> = conn.query_first(checksum_query).unwrap();

    match checksum {
        Some(checksum) => {
            let success_message = json!({
                "Status": 200,
                "Message": "Game Added Successfully",
                "GameId": gameid, 
                "Checksum": checksum,
            });
            Json(success_message)
        }
        None => {
            let error_message = json!({
                "Status": 500,
                "Message": "Failed to retrieve checksum",
            });
            Json(error_message)
        }
    }
}

#[get("/bot")]
fn get_random_bot() -> content::Json<String> {
    let connection = Connection::open("C:\\Users\\ntx\\Desktop\\apis\\apis\\bots.sqlite")
        .expect("Failed to open the database");

    let query = "SELECT cookie, password, otpkey FROM bots ORDER BY RANDOM() LIMIT 1;";
    let mut statement = connection.prepare(query).unwrap();

    let mut bot_data: Option<BotData> = None;

    while let Ok(State::Row) = statement.next() {
        let cookie = statement.read::<String, _>("cookie").unwrap();
        let password = statement.read::<String, _>("password").unwrap();
        let otpkey = statement.read::<String, _>("otpkey").unwrap();

        bot_data = Some(BotData { cookie, password, otpkey });
    }

    if let Some(bot_data) = bot_data {
        let json_response = serde_json::to_string(&bot_data).unwrap();
        return content::Json(json_response);
    }

    content::Json("".to_string())
}


#[get("/auth/<token>")]
fn generate_token(token: String) -> JsonValue {
    let secret: &String = &token;
    let totp: std::result::Result<TOTP, String> = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        Secret::Encoded(secret.to_string()).to_bytes().unwrap(),
    )
    .map_err(|err| err.to_string());

    if let Err(err) = totp {
        let error_message = json!({
            "Status": 400,
            "Message": err
        });
        return error_message.into();
    } else {
        let generated_token = totp.unwrap().generate_current().map_err(|err| err.to_string());
        if let Err(err) = generated_token {
            let error_message = json!({
                "Status": 400,
                "Message": err
            });
            return error_message.into();
        } else {
            let authtoken = json!({
                "Pin": generated_token.unwrap().to_string(),
                "Token": token
            });
            return authtoken.into();
        }
    }
}
#[get("/userinfo/<cookie>")]
fn get_user_info(cookie: String) -> JsonValue {
    let roblosecurity = &cookie;
    let client = Client::new();

    let mut headers = HeaderMap::new();
    headers.insert("Host", HeaderValue::from_static("auth.roblox.com"));
    headers.insert("Cookie", HeaderValue::from_str(&format!(".ROBLOSECURITY={}", roblosecurity)).unwrap());
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(CONTENT_LENGTH, HeaderValue::from_static("0"));

    let response = client
        .post("https://auth.roblox.com/v2/logout")
        .headers(headers.clone())
        .send()
        .unwrap();

    if let Some(csrf_token) = response.headers().get("x-csrf-token") {
        if let Ok(csrf_token_value) = csrf_token.to_str() {
            unsafe {
                CSRF = Some(csrf_token_value.to_string());
            }
        }
    }
   
    let mobileapi = client
        .get("https://www.roblox.com/mobileapi/userinfo")
        .header("Cookie", format!(".ROBLOSECURITY={}", roblosecurity))
        .send()
        .unwrap();

    if mobileapi.status().is_success() {
        let bodymobileapi = mobileapi.text().unwrap();
        let user_info: Result<UserInfo, serde_json::Error> = serde_json::from_str(&bodymobileapi);
        match user_info {
            Ok(user_info) => {
                let mut user_data: Map<String, Value> = Map::new();
                let headurl = format!("https://www.roblox.com/avatar-thumbnails?params=[{{\"userId\":\"{}\"}}]", user_info.UserID);
                let response = reqwest::blocking::get(&headurl).unwrap();
                let headdata: Vec<HeadShot> = response.json().unwrap();
                let mut rap = 0;
                let mut url = format!("https://inventory.roblox.com/v1/users/{}/assets/collectibles?sortOrder=Asc&limit=100",user_info.UserID).to_string();
                
                // Fetch user's inventory data and create the userAssetId array
                let mut user_asset_ids = Vec::new();
                let mut user_asset_info = Map::new();
                loop {
                    let response = reqwest::blocking::get(&url).unwrap().json::<serde_json::Value>().unwrap();
                    let response_json = response.as_object().unwrap();
                
                    if let Some(data) = response_json.get("data").and_then(|v| v.as_array()) {
                        rap += data.iter()
                            .filter_map(|item| item.get("recentAveragePrice").and_then(|v| v.as_u64()))
                            .sum::<u64>();

                        // Extract the userAssetIds and add them to the user_asset_ids array
                        for item in data.iter() {
                            if let Some(user_asset_id) = item.get("userAssetId").and_then(|v| v.as_u64()) {
                                user_asset_ids.push(user_asset_id);

                                // Add sub-JSON for the userAssetId
                                let asset_info = json!({
                                    "AssetId": item.get("assetId").and_then(|v| v.as_u64()).unwrap_or(0),
                                    "Onhold": item.get("isOnHold").and_then(|v| v.as_bool()).unwrap_or(true),
                                    "itemprice": item.get("recentAveragePrice").and_then(|v| v.as_u64()).unwrap_or(0),

                                });
                                

                                user_asset_info.insert(user_asset_id.to_string(), asset_info);

                            }
                        }
                    }
                
                    if let Some(next_page_cursor) = response_json.get("nextPageCursor").and_then(|v| v.as_str()) {
                        url = format!("{}&cursor={}", url.split("&cursor=").next().unwrap(), next_page_cursor);
                    } else {
                        break; // No nextPageCursor, exit loop
                    }
                }
                user_data.insert("UserRap".to_string(), Value::Number(serde_json::Number::from(rap)));

                if let Some(first_data) = headdata.get(0) {
                    user_data.insert("HeadShot".to_string(), Value::String(first_data.thumbnailUrl.clone()));
                }
                
                user_data.insert("UserID".to_string(), Value::Number(serde_json::Number::from(user_info.UserID)));
                user_data.insert("UserName".to_string(), Value::String(user_info.UserName));
                user_data.insert("RobuxBalance".to_string(), Value::Number(serde_json::Number::from(user_info.RobuxBalance)));
                user_data.insert("ThumbnailUrl".to_string(), Value::String(user_info.ThumbnailUrl));
                user_data.insert("IsAnyBuildersClubMember".to_string(), Value::Bool(user_info.IsAnyBuildersClubMember));
                user_data.insert("IsPremium".to_string(), Value::Bool(user_info.IsPremium));
                user_data.insert("Status".to_string(), Value::Number(serde_json::Number::from(200u32)));
                unsafe {
                    if let Some(ref csrf) = CSRF {
                        user_data.insert("Token".to_string(), Value::String(csrf.clone()));
                    }
                }

                // Insert the userAssetId array into the user_data map
                user_data.insert("UserAssetIds".to_string(), Value::Array(user_asset_ids.into_iter().map(|id| Value::Number(serde_json::Number::from(id))).collect()));
                
                // Insert the userAssetInfo sub-JSON into the user_data map
                user_data.insert("UserAssetInfo".to_string(), Value::Object(user_asset_info));

                return serde_json::to_value(user_data).unwrap().into();
            }
            Err(err) => {
                let mut user_data = Map::new();
                user_data.insert("Status".to_string(), Value::Number(serde_json::Number::from(400u32)));
                return serde_json::to_value(user_data).unwrap().into();
            }
        }
    } else {
        let mut user_data = Map::new();
        user_data.insert("Status".to_string(), Value::Number(serde_json::Number::from(400u32)));
        return serde_json::to_value(user_data).unwrap().into();
    }
}

fn generate_server_seed() -> String {
    let timestamp = chrono::Utc::now().timestamp_nanos();
    format!("{:?}", timestamp)
}

fn generate_client_seed() -> String {
    let mut rng = OsRng;
    let mut buffer = vec![0u8; 32];
    rng.fill_bytes(&mut buffer);
    let client_seed: String = buffer
        .iter()
        .map(|byte: &u8| format!("{:02x}", byte))
        .collect();

    client_seed
}

fn coinflip(server_seed: &str, client_seed: &str) -> f64 {
    let mut hmac: Hmac<Sha256> = Hmac::<Sha256>::new_varkey(server_seed.as_bytes()).expect("HMAC error");
    hmac.update(client_seed.as_bytes());
    let hash: hmac::digest::generic_array::GenericArray<u8, hmac::digest::generic_array::typenum::UInt<hmac::digest::generic_array::typenum::UInt<hmac::digest::generic_array::typenum::UInt<hmac::digest::generic_array::typenum::UInt<hmac::digest::generic_array::typenum::UInt<hmac::digest::generic_array::typenum::UInt<hmac::digest::generic_array::typenum::UTerm, hmac::digest::consts::B1>, hmac::digest::consts::B0>, hmac::digest::consts::B0>, hmac::digest::consts::B0>, hmac::digest::consts::B0>, hmac::digest::consts::B0>> = hmac.finalize().into_bytes();
    let numerator: u128 = u128::from_be_bytes(hash[..16].try_into().unwrap());
    let denominator: u128 = u128::MAX;
    if numerator > denominator {
        panic!("Numerator exceeds maximum value of u128.");
    }
    let mut result = numerator as f64 / denominator as f64;
    if result == 0.5 {
        let mut rng = rand::thread_rng();
        let random_number: f64 = rng.gen();
        result = if random_number < 0.5 { 0.6 } else { 0.4 };
    }
    result
}
#[get("/coinflip")]
fn flip_coin() -> content::Json<String> {
    let server_seed: String = generate_server_seed();
    let client_seed: String = generate_client_seed();
    let result: f64 = coinflip(&server_seed, &client_seed);
    let flipinfo: CoinflipResult = CoinflipResult {
        server_seed: server_seed.to_string(),
        client_seed,
        result,
    };
    let json_result = serde_json::to_string(&flipinfo).expect("JSON serialization error");
    content::Json(json_result)
}
#[get("/")]
fn core() -> rocket::response::content::Json<String> {
    let response: Server = Server {
        status: 200,
        message: String::from("BTC Flipping Is Down For Updates"),
    };
    rocket::response::content::Json(serde_json::to_string(&response).unwrap())
}



fn rocket() -> rocket::Rocket {
    rocket::ignite()
        .manage(create_db_pool())
        .mount("/", routes![get_user, add_user, get_random_bot, generate_token, get_user_info, flip_coin, core, update_user_auth_key, add_game, get_games, join_game,core_games])
}

fn create_db_pool() -> Pool {
    let url: &str = "mysql://root:admin@localhost:3306/flipusers";
    Pool::new(url).expect("Failed to create database pool")
}

fn main() {
    let broadcaster = create_broadcaster();
    set_broadcaster(broadcaster);
    if let Some(broadcaster) = unsafe { BROADCASTER.as_ref() } {
        broadcaster("Welcome :3".to_string());
    }
    rocket().launch();
}
