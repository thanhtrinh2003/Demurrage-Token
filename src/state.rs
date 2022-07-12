use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Uint128, Timestamp};
use cw_storage_plus::{Item, Map};

use cw20::{AllowanceResponse};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct TokenInfo {
    pub name: String,
    pub symbol: String,
    pub decimals: u32,
    pub total_supply: Uint128,
    pub mint: Option<MinterData>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct MinterData {
    pub minter: Addr,
    /// cap is how many more tokens can be issued by the minter
    pub cap: Option<Uint128>,
}

impl TokenInfo {
    pub fn get_cap(&self) -> Option<Uint128> {
        self.mint.as_ref().and_then(|v| v.cap)
    }
}


//demurrage state
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct State{   
    //timestamp of the initiation
    pub start_timestamp: Timestamp,
    ///timestamp of the initiation or from the last demurrage timestamp 
    pub demurrage_timestamp: Timestamp,
    ///number of minutes in one period 
    pub period_minute: u64, 
    /// current period count
    pub current_period: u64,
    pub demurrage_amount: u128,
    pub sink_address: String,
    pub minimum_participant_spend: u32,
    pub tax_level: u128,
}

impl State{
    pub fn get_current_period(&self) -> u64{
        return self.current_period;
    }
}


pub const TOKEN_INFO: Item<TokenInfo> = Item::new("token_info");
pub const BALANCES: Map<&Addr, Uint128> = Map::new("balance");
pub const ALLOWANCES: Map<(&Addr, &Addr), AllowanceResponse> = Map::new("allowance");

//demurrage state 
pub const STATE: Item<State> = Item::new("demurrage_state");
