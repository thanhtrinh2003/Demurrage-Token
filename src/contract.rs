#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, Timestamp, attr, Addr, BlockInfo, Storage
};

use cw2::set_contract_version;
use cw20::{
    BalanceResponse, Cw20Coin, Cw20ReceiveMsg, MinterResponse, TokenInfoResponse, Expiration, AllowanceResponse, DemurrageAmountResponse, 
    TaxLevelResponse, SinkAddressResponse
};

use crate::allowances::{
    execute_burn_from, execute_send_from,
    query_allowance,
};
use crate::enumerable::{query_all_accounts, query_all_allowances};
use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{MinterData, TokenInfo, BALANCES, TOKEN_INFO, State, STATE, ALLOWANCES};


// version info for migration info
const CONTRACT_NAME: &str = "crates.io:cw20-base";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const NANO_DIVDER: u128 = 10000000000000;  //10^13
const GROWTH_RESOLUTION_FACTOR: u128 = 10000000; // 10^7


//10^39
const RESOLUTION_FACTOR: u128 = NANO_DIVDER * GROWTH_RESOLUTION_FACTOR; //this value may get out of bound


/*
    *******************
    *******************
    *******************
    INITIATION PART 
    *******************
    *******************
    *******************
    *******************
*/



#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,

) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    // check valid token info
    msg.validate()?;
    // create initial accounts
    let total_supply = create_accounts(&mut deps, &msg.initial_balances)?;


    // Demurrage Setup 
    let period_start = _env.block.time;
    let period_duration = msg.period_minutes * 60;
    //demurrageAmount = 100000000000000000000000000000000000000 - _taxLevelMinute; // Represents 38 decimal places, same as resolutionFactor
    //demurrageAmount = 100000000000000000000000000000000000000;
    let demurrage_amount: u128 = 1000000000000000; //10^15
    //demurragePeriod = 1;
    let tax_level = msg.tax_level_minute;
    let base_ten: u32 = 10;
    

    if let Some(limit) = msg.get_cap() {
        if total_supply > limit {
            return Err(StdError::generic_err("Initial supply greater than cap").into());
        }
    }

    let sink_addr = msg.sink_address;

    //saving 
    let state = State{
        start_timestamp: period_start,
        demurrage_timestamp : period_start, 
        period_minute: period_duration, 
        current_period: 0,
        demurrage_amount: demurrage_amount,
        sink_address: sink_addr, 
        minimum_participant_spend: base_ten.pow(msg.decimals),
        tax_level: tax_level,
    };
    STATE.save(deps.storage, &state)?;

    let mint = match msg.mint {
        Some(m) => Some(MinterData {
            minter: deps.api.addr_validate(&m.minter)?,
            cap: m.cap,
        }),
        None => None,
    };

    // store token info
    let data = TokenInfo {
        name: msg.name,
        symbol: msg.symbol,
        decimals: msg.decimals,
        total_supply,
        mint,
    };
    TOKEN_INFO.save(deps.storage, &data)?;

    Ok(Response::default())
}

pub fn create_accounts(
    deps: &mut DepsMut,
    accounts: &[Cw20Coin],
) -> Result<Uint128, ContractError> {
    validate_accounts(accounts)?;

    let mut total_supply = Uint128::zero();
    for row in accounts {
        let address = deps.api.addr_validate(&row.address)?;
        BALANCES.save(deps.storage, &address, &row.amount)?;
        total_supply += row.amount;
    }

    Ok(total_supply)
}

pub fn validate_accounts(accounts: &[Cw20Coin]) -> Result<(), ContractError> {
    let mut addresses = accounts.iter().map(|c| &c.address).collect::<Vec<_>>();
    addresses.sort();
    addresses.dedup();

    if addresses.len() != accounts.len() {
        Err(ContractError::DuplicateInitialBalanceAddresses {})
    } else {
        Ok(())
    }
}


/*
    *******************
    *******************
    *******************
    EXECUTE FUNCTIONS
    *******************
    *******************
    *******************
    *******************
*/


#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Transfer { recipient, amount } => {
            execute_transfer(deps, env, info, recipient, amount)
        }
        ExecuteMsg::Burn { amount } => execute_burn(deps, env, info, amount),
        ExecuteMsg::Send {
            contract,
            amount,
            msg,
        } => execute_send(deps, env, info, contract, amount, msg),
        ExecuteMsg::Mint { recipient, amount } => execute_mint(deps, env, info, recipient, amount),
        ExecuteMsg::IncreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_increase_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::DecreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_decrease_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => execute_transfer_from(deps, env, info, owner, recipient, amount),
        ExecuteMsg::BurnFrom { owner, amount } => execute_burn_from(deps, env, info, owner, amount),
        ExecuteMsg::SendFrom {
            owner,
            contract,
            amount,
            msg,
        } => execute_send_from(deps, env, info, owner, contract, amount, msg),
        ExecuteMsg::UpdateMinter { new_minter } => {
            execute_update_minter(deps, env, info, new_minter)
        },
        ExecuteMsg::ChangeSinkAddress{ address } => {
            execute_change_sink_address(deps, address)
        }
        ExecuteMsg::ChangeTaxLevel{ amount } => {
            execute_change_tax_level(deps, amount)
        }
    }
}

pub fn execute_change_tax_level(
    mut deps: DepsMut, 
    amount: u128, 
) -> Result<Response, ContractError>{

    let mut state = STATE
    .may_load(deps.storage)?
    .ok_or(ContractError::Unauthorized {})?;

    state.tax_level = amount;
    STATE.save(deps.storage, &state)?;

    let res = Response::new()
        .add_attribute("action", "change tax level")
        .add_attribute("new tax amount", Uint128::from(state.tax_level));
    Ok(res)
}

pub fn execute_change_sink_address(
    mut deps: DepsMut, 
    sink_addr: String, 
) -> Result<Response, ContractError>{

    let mut state = STATE
    .may_load(deps.storage)?
    .ok_or(ContractError::Unauthorized {})?;

    state.sink_address = sink_addr;
    STATE.save(deps.storage, &state)?;

    let res = Response::new()
        .add_attribute("action", "change sink address")
        .add_attribute("new address", state.sink_address);
    Ok(res)
}

pub fn execute_transfer(
    mut deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {

    let mut state = STATE
    .may_load(deps.storage)?
    .ok_or(ContractError::Unauthorized {})?;
    change_period(&mut deps, _env, &mut state);

   let base_value: u128; 
    base_value = to_base_amount(amount.u128(), state.demurrage_amount);

    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let rcpt_addr = deps.api.addr_validate(&recipient)?;

    BALANCES.update(
        deps.storage,
        &info.sender,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + Uint128::from(base_value)) },
    )?;

    let res = Response::new()
        .add_attribute("action", "transfer")
        .add_attribute("from", info.sender)
        .add_attribute("to", recipient)
        .add_attribute("amount", amount);
    Ok(res)
}

pub fn execute_transfer_from(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let rcpt_addr = deps.api.addr_validate(&recipient)?;
    let owner_addr = deps.api.addr_validate(&owner)?;

    // deduct allowance before doing anything else have enough allowance
    deduct_allowance(deps.storage, &owner_addr, &info.sender, &env.block, amount)?;

    let mut state = STATE
    .may_load(deps.storage)?
    .ok_or(ContractError::Unauthorized {})?;

    let base_value: u128;

    change_period(&mut deps, env,  &mut state);

    base_value = to_base_amount(amount.u128(), state.demurrage_amount);


    BALANCES.update(
        deps.storage,
        &owner_addr,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + Uint128::from(base_value)) },
    )?;

    let res = Response::new()
        .add_attribute("action", "transfer")
        .add_attribute("from", owner)
        .add_attribute("to", recipient)
        .add_attribute("amount", amount);
    Ok(res)
}

pub fn execute_burn(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    // lower balance
    BALANCES.update(
        deps.storage,
        &info.sender,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    // reduce total_supply
    TOKEN_INFO.update(deps.storage, |mut info| -> StdResult<_> {
        info.total_supply = info.total_supply.checked_sub(amount)?;
        Ok(info)
    })?;

    let res = Response::new()
        .add_attribute("action", "burn")
        .add_attribute("from", info.sender)
        .add_attribute("amount", amount);
    Ok(res)
}

pub fn execute_mint(
    mut deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
    
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let mut config = TOKEN_INFO
        .may_load(deps.storage)?
        .ok_or(ContractError::Unauthorized {})?;

    let mut state = STATE
    .may_load(deps.storage)?
    .ok_or(ContractError::Unauthorized {})?;


    if config
        .mint
        .as_ref()
        .ok_or(ContractError::Unauthorized {})?
        .minter
        != info.sender
    {
        return Err(ContractError::Unauthorized {});
    }

    // update supply and enforce cap
    config.total_supply += amount;
    if let Some(limit) = config.get_cap() {
        if config.total_supply > limit {
            return Err(ContractError::CannotExceedCap {});
        }
    }
    TOKEN_INFO.save(deps.storage, &config)?;

    change_period(&mut deps, _env, &mut state); 

    let base_value : u128;
    base_value = to_base_amount(amount.u128(), state.demurrage_amount);


    // add amount to recipient balance
    let rcpt_addr = deps.api.addr_validate(&recipient)?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + Uint128::from(base_value)) },
    )?;

    let res = Response::new()
        .add_attribute("action", "mint")
        .add_attribute("to", recipient)
        .add_attribute("amount", amount);
    Ok(res)
}

pub fn execute_send(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    contract: String,
    amount: Uint128,
    msg: Binary,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let rcpt_addr = deps.api.addr_validate(&contract)?;

    // move the tokens to the contract
    BALANCES.update(
        deps.storage,
        &info.sender,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new()
        .add_attribute("action", "send")
        .add_attribute("from", &info.sender)
        .add_attribute("to", &contract)
        .add_attribute("amount", amount)
        .add_message(
            Cw20ReceiveMsg {
                sender: info.sender.into(),
                amount,
                msg,
            }
            .into_cosmos_msg(contract)?,
        );
    Ok(res)
}

pub fn execute_update_minter(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    new_minter: String,
) -> Result<Response, ContractError> {
    let mut config = TOKEN_INFO
        .may_load(deps.storage)?
        .ok_or(ContractError::Unauthorized {})?;

    let mint = config.mint.as_ref().ok_or(ContractError::Unauthorized {})?;
    if mint.minter != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    let minter = deps.api.addr_validate(&new_minter)?;
    let minter_data = MinterData {
        minter,
        cap: mint.cap,
    };
    config.mint = Some(minter_data);

    TOKEN_INFO.save(deps.storage, &config)?;

    Ok(Response::default()
        .add_attribute("action", "update_minter")
        .add_attribute("new_minter", new_minter))
}

//ALLOWANCE:__rust_force_expr!
pub fn execute_increase_allowance(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    spender: String,
    amount: Uint128,
    expires: Option<Expiration>,
) -> Result<Response, ContractError> {
    let spender_addr = deps.api.addr_validate(&spender)?;
    if spender_addr == info.sender {
        return Err(ContractError::CannotSetOwnAccount {});
    }

    let mut state = STATE
    .may_load(deps.storage)?
    .ok_or(ContractError::Unauthorized {})?;

    let base_value: u128;
    base_value = to_base_amount(amount.u128(), state.demurrage_amount);

    ALLOWANCES.update(
        deps.storage,
        (&info.sender, &spender_addr),
        |allow| -> StdResult<_> {
            let mut val = allow.unwrap_or_default();
            if let Some(exp) = expires {
                val.expires = exp;
            }
            val.allowance += amount + Uint128::from(base_value);
            Ok(val)
        },
    )?;

    let res = Response::new().add_attributes(vec![
        attr("action", "increase_allowance"),
        attr("owner", info.sender),
        attr("spender", spender),
        attr("amount", amount),
    ]);
    Ok(res)
}

pub fn execute_decrease_allowance(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    spender: String,
    amount: Uint128,
    expires: Option<Expiration>,
) -> Result<Response, ContractError> {
    let spender_addr = deps.api.addr_validate(&spender)?;
    if spender_addr == info.sender {
        return Err(ContractError::CannotSetOwnAccount {});
    }

    let key = (&info.sender, &spender_addr);
    // load value and delete if it hits 0, or update otherwise
    let mut allowance = ALLOWANCES.load(deps.storage, key)?;
    if amount < allowance.allowance {
        // update the new amount
        allowance.allowance = allowance
            .allowance
            .checked_sub(amount)
            .map_err(StdError::overflow)?;
        if let Some(exp) = expires {
            allowance.expires = exp;
        }
        ALLOWANCES.save(deps.storage, key, &allowance)?;
    } else {
        ALLOWANCES.remove(deps.storage, key);
    }

    let res = Response::new().add_attributes(vec![
        attr("action", "decrease_allowance"),
        attr("owner", info.sender),
        attr("spender", spender),
        attr("amount", amount),
    ]);
    Ok(res)
}

// this can be used to update a lower allowance - call bucket.update with proper keys
pub fn deduct_allowance(
    storage: &mut dyn Storage,
    owner: &Addr,
    spender: &Addr,
    block: &BlockInfo,
    amount: Uint128,
) -> Result<AllowanceResponse, ContractError> {
    ALLOWANCES.update(storage, (owner, spender), |current| {
        match current {
            Some(mut a) => {
                if a.expires.is_expired(block) {
                    Err(ContractError::Expired {})
                } else {
                    // deduct the allowance if enough
                    a.allowance = a
                        .allowance
                        .checked_sub(amount)
                        .map_err(StdError::overflow)?;
                    Ok(a)
                }
            }
            None => Err(ContractError::NoAllowance {}),
        }
    })
}


/*
    *******************
    *******************
    *******************
    DEMURRAGE FUNCTIONS
    *******************
    *******************
    *******************
    *******************
*/

/// Apply Default Redistribution: all amounts go to sink address
pub fn apply_default_redistribution(
    deps: &mut DepsMut,
    state: &mut State,
    distribution: u128, 
) -> Result<Response, ContractError> {
    let sink_addr = deps.api.addr_validate(&state.sink_address)?;

    BALANCES.update(
        deps.storage,
        &sink_addr,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + Uint128::from(distribution))},
    )?;  

    let res = Response::new()
    .add_attribute("action", "default_redistribution")
    .add_attribute("amount", distribution.to_string());
    
    Ok(res)

    //BALANCES.update(
    //     deps.storage,
    //     &info.sender,
    //     |balance: Option<Uint128>| -> StdResult<_> {
    //         Ok(balance.unwrap_or_default().checked_sub(amount)?)
    //     },
    // )?;
    // BALANCES.update(
    //     deps.storage,
    //     &rcpt_addr,
    //     |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + Uint128::from(base_value)) },
    // )?;

    // let res = Response::new()
    //     .add_attribute("action", "transfer")
    //     .add_attribute("from", info.sender)
    //     .add_attribute("to", recipient)
    //     .add_attribute("amount", amount);
    // Ok(res)
}


///Amount of demurrage cycles inbetween the current timestamp and the given target time
pub fn demurrage_cycles(
    now_timestamp: Timestamp, // _env.block.time.
    target: Timestamp,
) -> u64 {
    return (now_timestamp.seconds() - target.seconds())/60;
}


///Recalculate the demurrage modifier for the new period
pub fn change_period(
    deps: &mut DepsMut, 
    _env: Env, 
    state: &mut State,
) -> Result<bool, ContractError> {
    //take current timestamp
    let current_timestamp: Timestamp = _env.block.time;
    println!("change period, current timestamp: {}", current_timestamp);
    println!("change period, start_timestamp timestamp: {}", state.start_timestamp);

    //decrease demurrage amount 
    apply_demurrage(deps, current_timestamp, state);

    println!("state.current_period: {}", state.current_period);
    
    let next_period: u64 = state.current_period + 1;

    println!("state.current_period: {}", state.current_period);
    println!("period_minute: {}", state.period_minute);
    println!("start_timestamp: {}", state.start_timestamp);

    let period_timestamp: Timestamp = get_period_time_delta(state.start_timestamp, state.current_period, state.period_minute);
    
    println!("current timestamp: {}", current_timestamp.seconds());
    println!("period timestamp: {} ", period_timestamp.seconds());
    let current_demurrage_amount: u128 = state.demurrage_amount;
    let demurrage_counts: u64= demurrage_cycles(current_timestamp, period_timestamp);
    
    
    println!("different between: {} ", current_timestamp.seconds() - period_timestamp.seconds());


    let next_demurrage_amount: u128;
    if demurrage_counts > 0
    {
        next_demurrage_amount = grow_by(current_demurrage_amount, state.tax_level, demurrage_counts);
    }
    else
    {
        next_demurrage_amount = current_demurrage_amount;
    }

    //update the demurrage state 
    state.demurrage_amount = next_demurrage_amount;
    state.current_period = next_period;
    STATE.save(deps.storage, &state);

    //send the tax to the sink account 
    let distribution = get_distribution(deps, state)?;
    let res = apply_default_redistribution(deps, state, distribution);

    return Ok(true);
}


///Get the demurrage period of the current block number
pub fn actual_period(
    now_timestamp: Timestamp, // _env.block.time
    state: &mut State,
)-> u128 {
    return u128::from((now_timestamp.seconds()- state.start_timestamp.seconds())/60 / state.period_minute + 1);
}

/// Get Distribution Function
pub fn get_distribution(
    deps: &mut DepsMut, 
    state: &mut State,
) -> Result<u128, ContractError> {
    let config = TOKEN_INFO
    .may_load(deps.storage)?
    .ok_or(ContractError::Unauthorized {})?;

    
    //testing
    println!("get_distribution, total supply: {}", config.total_supply.u128());
    println!("get_distribution, demurrage_amount: {}", state.demurrage_amount);


    let difference : u128;
    difference = config.total_supply.u128() * (RESOLUTION_FACTOR- (state.demurrage_amount * 100000));
    
    
    return Ok(difference/ RESOLUTION_FACTOR);
}

///Default apply demurrage function, no limitations on number of periods 
///Refer to execute_apply_demurrage_limited for more information
pub fn apply_demurrage(
    deps: &mut DepsMut, 
    now_timestamp: Timestamp,
    state: &mut State,
) -> bool{
    return apply_demurrage_limited(deps, now_timestamp, state, 0);
}

/// Calculate and cache the demurrage value correpsonding to the (period of the)
/// time of the methdo call
pub fn apply_demurrage_limited(
    deps: &mut DepsMut, 
    now_timestamp: Timestamp,
    state: &mut State,
    rounds: u64,
) -> bool{
    let mut period_count: u64;
    let last_demurrage_amount: u128;
    
    //determine how many periods between the last time applied demurrage and the current time.
    period_count = get_minutes_delta(now_timestamp, state.demurrage_timestamp);

    println!("apply demurrage, period count: {}", period_count);

    if period_count == 0
    {
        return false;
    }

    // safety limit for exponential calculation to ensure that we can always
	// execute this code no matter how much time passes.	
    if rounds > 0 && rounds < period_count
    {
        period_count = rounds;
    }


    // demurrage amount decayed according to the number of period
    last_demurrage_amount = decay_by(state.demurrage_amount, state.tax_level, period_count);
    
    //update state 
    state.demurrage_amount = last_demurrage_amount;
    state.demurrage_timestamp.plus_seconds(period_count * 60);
    STATE.save(deps.storage, &state);

    return true;
}

/// Return timestamp of start of period threshold
fn get_period_time_delta (
    start_timestamp: Timestamp,
    period_count: u64,
    period_minute: u64
) -> Timestamp {
    return start_timestamp.plus_seconds(period_count * period_minute);
}


//check please if it needs to fix the 100000000 value
/// Inflates the given amount according to the current demurrage modifier
fn to_base_amount(
    value: u128,
    demurrage_amount: u128
)-> u128{
    return value * RESOLUTION_FACTOR / (demurrage_amount * 100000)
}


/// Calculate the time delta in whole minutes passed between given timestamp and current timestamp
fn get_minutes_delta (
    now_timestamp: Timestamp, 
    last_timestamp: Timestamp
) -> u64{
    return (now_timestamp.seconds() - last_timestamp.seconds())/60
}

fn grow_by (
    value: u128, 
    tax_level: u128,
    period: u64, 
) -> u128 {
    let mut value_factor: u128; 
    let truncated_tax_level:u128; 

    value_factor = GROWTH_RESOLUTION_FACTOR;
    truncated_tax_level = tax_level / NANO_DIVDER;

    for n in 1..period {
        value_factor = (value_factor + ((value_factor * truncated_tax_level)/GROWTH_RESOLUTION_FACTOR)) as u128;
    }
    return (value_factor * value) / GROWTH_RESOLUTION_FACTOR;
}

fn decay_by (
    value: u128, 
    tax_level: u128,
    period: u64, 
) -> u128 {
    let mut value_factor: u128; 
    let truncated_tax_level:u128; 

    value_factor = GROWTH_RESOLUTION_FACTOR;
    truncated_tax_level = tax_level / NANO_DIVDER;

    for n in 1..(period+1) {
        value_factor = (value_factor - ((value_factor * truncated_tax_level)/GROWTH_RESOLUTION_FACTOR)) as u128;
        println!("here");
    }

    return (value_factor * value)/ GROWTH_RESOLUTION_FACTOR;
}


/*
    *******************
    *******************
    *******************
    QUERY FUNCTIONS
    *******************
    *******************
    *******************
    *******************
*/



#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Balance { address } => to_binary(&query_balance(deps, address)?),
        QueryMsg::TokenInfo {} => to_binary(&query_token_info(deps)?),
        QueryMsg::Minter {} => to_binary(&query_minter(deps)?),
        QueryMsg::Allowance { owner, spender } => {
            to_binary(&query_allowance(deps, owner, spender)?)
        }
        QueryMsg::AllAllowances {
            owner,
            start_after,
            limit,
        } => to_binary(&query_all_allowances(deps, owner, start_after, limit)?),
        QueryMsg::AllAccounts { start_after, limit } => {
            to_binary(&query_all_accounts(deps, start_after, limit)?)
        }

        QueryMsg::DemurrageAmount {} => to_binary(&query_demurrage_amount(deps)?),
        QueryMsg::TaxLevel {} => to_binary(&query_tax_level(deps)?), 
        QueryMsg::SinkAccount {} => to_binary(&query_sink_address(deps)?), 
        
    }
}

pub fn query_tax_level(deps: Deps) -> StdResult<TaxLevelResponse> {
    let state = STATE.load(deps.storage)?;
    let tax_level = Uint128::from(state.tax_level);

    Ok(TaxLevelResponse {tax_level})
}

pub fn query_sink_address(deps: Deps) -> StdResult<SinkAddressResponse> {
    let state = STATE.load(deps.storage)?;
    let sink_address = state.sink_address;

    Ok(SinkAddressResponse {sink_address})
}

pub fn query_balance(deps: Deps, address: String) -> StdResult<BalanceResponse> {
    let address = deps.api.addr_validate(&address)?;
    let balance = BALANCES
        .may_load(deps.storage, &address)?
        .unwrap_or_default();
    Ok(BalanceResponse { balance })
}

pub fn query_token_info(deps: Deps) -> StdResult<TokenInfoResponse> {
    let info = TOKEN_INFO.load(deps.storage)?;
    let res = TokenInfoResponse {
        name: info.name,
        symbol: info.symbol,
        decimals: info.decimals,
        total_supply: info.total_supply,
    };
    Ok(res)
}

pub fn query_minter(deps: Deps) -> StdResult<Option<MinterResponse>> {
    let meta = TOKEN_INFO.load(deps.storage)?;
    let minter = match meta.mint {
        Some(m) => Some(MinterResponse {
            minter: m.minter.into(),
            cap: m.cap,
        }),
        None => None,
    };
    Ok(minter)
}

pub fn query_demurrage_amount(deps: Deps) -> StdResult<DemurrageAmountResponse>{
    let state = STATE.load(deps.storage)?;
    let demurrage_amount :Uint128 = Uint128::from(state.demurrage_amount);
    
    Ok(DemurrageAmountResponse{demurrage_amount})
}




#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{
        mock_dependencies, mock_dependencies_with_balance, mock_env, mock_info,
    };
    use cosmwasm_std::{coins, from_binary, Addr, CosmosMsg, StdError, SubMsg, WasmMsg};

    use super::*;
    use crate::msg::InstantiateMarketingInfo;

    fn get_balance<T: Into<String>>(deps: Deps, address: T) -> Uint128 {
        query_balance(deps, address.into()).unwrap().balance
    }

    // this will set up the instantiation for other tests
    fn do_instantiate_with_minter(
        deps: DepsMut,
        addr: &str,
        amount: Uint128,
        minter: &str,
        cap: Option<Uint128>,
    ) -> TokenInfoResponse {
        _do_instantiate(
            deps,
            addr,
            amount,
            Some(MinterResponse {
                minter: minter.to_string(),
                cap,
            }),
        )
    }

    // this will set up the instantiation for other tests
    fn do_instantiate(deps: DepsMut, addr: &str, amount: Uint128) -> TokenInfoResponse {
        _do_instantiate(deps, addr, amount, None)
    }

    // this will set up the instantiation for other tests
    fn _do_instantiate(
        mut deps: DepsMut,
        addr: &str,
        amount: Uint128,
        mint: Option<MinterResponse>,
    ) -> TokenInfoResponse {
        let instantiate_msg = InstantiateMsg {
            name: "Auto Gen".to_string(),
            symbol: "AUTO".to_string(),
            decimals: 3,
            initial_balances: vec![Cw20Coin {
                address: addr.to_string(),
                amount,
            }],
            mint: mint.clone(),
            
            //0.000050105908373373
            tax_level_minute: 50105908373373, // 38 decimals
            period_minutes: 1, 
            supply_cap: 10000000, //supply cap is 10 million coins
            sink_address: "sinkaddress".to_string()

        };
        let info = mock_info("creator", &[]);
        let env = mock_env();
        let res = instantiate(deps.branch(), env, info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        let meta = query_token_info(deps.as_ref()).unwrap();
        assert_eq!(
            meta,
            TokenInfoResponse {
                name: "Auto Gen".to_string(),
                symbol: "AUTO".to_string(),
                decimals: 3,
                total_supply: amount,
            }
        );
        assert_eq!(get_balance(deps.as_ref(), addr), amount);
        assert_eq!(query_minter(deps.as_ref()).unwrap(), mint,);
        meta
    }

    mod instantiate {
        use super::*;

        #[test]
        fn basic() {
            let mut deps = mock_dependencies();
            let amount = Uint128::from(11223344u128);
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![Cw20Coin {
                    address: String::from("addr0000"),
                    amount,
                }],
                mint: None,
                tax_level_minute: 50105908373373, // 38 decimals
                period_minutes: 1, 
                supply_cap: 10000000, //supply cap is 10 million coins
                sink_address: "sinkaddress".to_string()
            };
            let info = mock_info("creator", &[]);
            let env = mock_env();
            let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
            assert_eq!(0, res.messages.len());

            assert_eq!(
                query_token_info(deps.as_ref()).unwrap(),
                TokenInfoResponse {
                    name: "Cash Token".to_string(),
                    symbol: "CASH".to_string(),
                    decimals: 9,
                    total_supply: amount,
                }
            );
            assert_eq!(
                get_balance(deps.as_ref(), "addr0000"),
                Uint128::new(11223344)
            );
        }

        #[test]
        fn mintable() {
            let mut deps = mock_dependencies();
            let amount = Uint128::new(11223344);
            let minter = String::from("asmodat");
            let limit = Uint128::new(511223344);
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![Cw20Coin {
                    address: "addr0000".into(),
                    amount,
                }],
                mint: Some(MinterResponse {
                    minter: minter.clone(),
                    cap: Some(limit),
                }),
                tax_level_minute: 50105908373373, // 38 decimals
                period_minutes: 1, 
                supply_cap: 10000000, //supply cap is 10 million coins
                sink_address: "sinkaddress".to_string(),
            };
            let info = mock_info("creator", &[]);
            let env = mock_env();
            let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
            assert_eq!(0, res.messages.len());

            assert_eq!(
                query_token_info(deps.as_ref()).unwrap(),
                TokenInfoResponse {
                    name: "Cash Token".to_string(),
                    symbol: "CASH".to_string(),
                    decimals: 9,
                    total_supply: amount,
                }
            );
            assert_eq!(
                get_balance(deps.as_ref(), "addr0000"),
                Uint128::new(11223344)
            );
            assert_eq!(
                query_minter(deps.as_ref()).unwrap(),
                Some(MinterResponse {
                    minter,
                    cap: Some(limit),
                }),
            );
        }

        #[test]
        fn mintable_over_cap() {
            let mut deps = mock_dependencies();
            let amount = Uint128::new(11223344);
            let minter = String::from("asmodat");
            let limit = Uint128::new(11223300);
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![Cw20Coin {
                    address: String::from("addr0000"),
                    amount,
                }],
                mint: Some(MinterResponse {
                    minter,
                    cap: Some(limit),
                }),
                tax_level_minute: 50105908373373, // 38 decimals
                period_minutes: 1, 
                supply_cap: 10000000, //supply cap is 10 million coins
                sink_address: "sinkaddress".to_string()
               
            };
            let info = mock_info("creator", &[]);
            let env = mock_env();
            let err = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap_err();
            assert_eq!(
                err,
                StdError::generic_err("Initial supply greater than cap").into()
            );
        }
    }

    #[test]
    fn can_mint_by_minter() {
        let mut deps = mock_dependencies();

        let genesis = String::from("genesis");
        let amount = Uint128::new(11223344);
        let minter = String::from("asmodat");
        let limit = Uint128::new(511223344);
        do_instantiate_with_minter(deps.as_mut(), &genesis, amount, &minter, Some(limit));

        // minter can mint coins to some winner
        let winner = String::from("lucky");
        let prize = Uint128::new(222_222_222);
        let msg = ExecuteMsg::Mint {
            recipient: winner.clone(),
            amount: prize,
        };

        let info = mock_info(minter.as_ref(), &[]);
        let env = mock_env();
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(get_balance(deps.as_ref(), genesis), amount);
        assert_eq!(get_balance(deps.as_ref(), winner.clone()), prize);

        // but cannot mint nothing
        let msg = ExecuteMsg::Mint {
            recipient: winner.clone(),
            amount: Uint128::zero(),
        };
        let info = mock_info(minter.as_ref(), &[]);
        let env = mock_env();
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});

        // but if it exceeds cap (even over multiple rounds), it fails
        // cap is enforced
        let msg = ExecuteMsg::Mint {
            recipient: winner,
            amount: Uint128::new(333_222_222),
        };
        let info = mock_info(minter.as_ref(), &[]);
        let env = mock_env();
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::CannotExceedCap {});
    }

    #[test]
    fn others_cannot_mint() {
        let mut deps = mock_dependencies();
        do_instantiate_with_minter(
            deps.as_mut(),
            &String::from("genesis"),
            Uint128::new(1234),
            &String::from("minter"),
            None,
        );

        let msg = ExecuteMsg::Mint {
            recipient: String::from("lucky"),
            amount: Uint128::new(222),
        };
        let info = mock_info("anyone else", &[]);
        let env = mock_env();
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn minter_can_update_minter_but_not_cap() {
        let mut deps = mock_dependencies();
        let minter = String::from("minter");
        let cap = Some(Uint128::from(3000000u128));
        do_instantiate_with_minter(
            deps.as_mut(),
            &String::from("genesis"),
            Uint128::new(1234),
            &minter,
            cap,
        );

        let new_minter = "new_minter";
        let msg = ExecuteMsg::UpdateMinter {
            new_minter: new_minter.to_string(),
        };

        let info = mock_info(&minter, &[]);
        let env = mock_env();
        let res = execute(deps.as_mut(), env.clone(), info, msg);
        assert!(res.is_ok());
        let query_minter_msg = QueryMsg::Minter {};
        let res = query(deps.as_ref(), env, query_minter_msg);
        let mint: MinterResponse = from_binary(&res.unwrap()).unwrap();

        // Minter cannot update cap.
        assert!(mint.cap == cap);
        assert!(mint.minter == new_minter)
    }

    #[test]
    fn others_cannot_update_minter() {
        let mut deps = mock_dependencies();
        let minter = String::from("minter");
        do_instantiate_with_minter(
            deps.as_mut(),
            &String::from("genesis"),
            Uint128::new(1234),
            &minter,
            None,
        );

        let msg = ExecuteMsg::UpdateMinter {
            new_minter: String::from("new_minter"),
        };

        let info = mock_info("not the minter", &[]);
        let env = mock_env();
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn no_one_mints_if_minter_unset() {
        let mut deps = mock_dependencies();
        do_instantiate(deps.as_mut(), &String::from("genesis"), Uint128::new(1234));

        let msg = ExecuteMsg::Mint {
            recipient: String::from("lucky"),
            amount: Uint128::new(222),
        };
        let info = mock_info("genesis", &[]);
        let env = mock_env();
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn instantiate_multiple_accounts() {
        let mut deps = mock_dependencies();
        let amount1 = Uint128::from(11223344u128);
        let addr1 = String::from("addr0001");
        let amount2 = Uint128::from(7890987u128);
        let addr2 = String::from("addr0002");
        let info = mock_info("creator", &[]);
        let env = mock_env();

        // Fails with duplicate addresses
        let instantiate_msg = InstantiateMsg {
            name: "Bash Shell".to_string(),
            symbol: "BASH".to_string(),
            decimals: 6,
            initial_balances: vec![
                Cw20Coin {
                    address: addr1.clone(),
                    amount: amount1,
                },
                Cw20Coin {
                    address: addr1.clone(),
                    amount: amount2,
                },
            ],
            mint: None,
            tax_level_minute: 50105908373373, // 38 decimals
            period_minutes: 1, 
            supply_cap: 10000000, //supply cap is 10 million coins
            sink_address: "sinkaddress".to_string()
           
        };
        let err =
            instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap_err();
        assert_eq!(err, ContractError::DuplicateInitialBalanceAddresses {});

        // Works with unique addresses
        let instantiate_msg = InstantiateMsg {
            name: "Bash Shell".to_string(),
            symbol: "BASH".to_string(),
            decimals: 6,
            initial_balances: vec![
                Cw20Coin {
                    address: addr1.clone(),
                    amount: amount1,
                },
                Cw20Coin {
                    address: addr2.clone(),
                    amount: amount2,
                },
            ],
            mint: None,
            tax_level_minute: 50105908373373, // 38 decimals
            period_minutes: 1, 
            supply_cap: 10000000, //supply cap is 10 million coins
            sink_address: "sinkaddress".to_string()
            
        };
        let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap(),
            TokenInfoResponse {
                name: "Bash Shell".to_string(),
                symbol: "BASH".to_string(),
                decimals: 6,
                total_supply: amount1 + amount2,
            }
        );
        assert_eq!(get_balance(deps.as_ref(), addr1), amount1);
        assert_eq!(get_balance(deps.as_ref(), addr2), amount2);
    }

    #[test]
    fn queries_work() {
        let mut deps = mock_dependencies_with_balance(&coins(2, "token"));
        let addr1 = String::from("addr0001");
        let amount1 = Uint128::from(12340000u128);

        let expected = do_instantiate(deps.as_mut(), &addr1, amount1);

        // check meta query
        let loaded = query_token_info(deps.as_ref()).unwrap();
        assert_eq!(expected, loaded);

        let _info = mock_info("test", &[]);
        let env = mock_env();
        // check balance query (full)
        let data = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::Balance { address: addr1 },
        )
        .unwrap();
        let loaded: BalanceResponse = from_binary(&data).unwrap();
        assert_eq!(loaded.balance, amount1);

        // check balance query (empty)
        let data = query(
            deps.as_ref(),
            env,
            QueryMsg::Balance {
                address: String::from("addr0002"),
            },
        )
        .unwrap();
        let loaded: BalanceResponse = from_binary(&data).unwrap();
        assert_eq!(loaded.balance, Uint128::zero());
    }

    #[test]
    fn transfer() {
        let mut deps = mock_dependencies_with_balance(&coins(2, "token"));
        let addr1 = String::from("addr0001");
        let addr2 = String::from("addr0002");
        let amount1 = Uint128::from(12340000u128);
        let transfer = Uint128::from(76543u128);
        let too_much = Uint128::from(12340321u128);

        do_instantiate(deps.as_mut(), &addr1, amount1);

        //start time is 1571797419 from instantiate

        // cannot transfer nothing
        let info = mock_info(addr1.as_ref(), &[]);
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(1571797479);
        
        let msg = ExecuteMsg::Transfer {
            recipient: addr2.clone(),
            amount: Uint128::zero(),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});

        // cannot send more than we have
        let info = mock_info(addr1.as_ref(), &[]);
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(1571797539);

        let msg = ExecuteMsg::Transfer {
            recipient: addr2.clone(),
            amount: too_much,
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));

        // cannot send from empty account
        let info = mock_info(addr2.as_ref(), &[]);
        let mut env = mock_env();

        env.block.time = Timestamp::from_seconds(1571797659);
        let msg = ExecuteMsg::Transfer {
            recipient: addr1.clone(),
            amount: transfer,
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));

        // valid transfer
        let info = mock_info(addr1.as_ref(), &[]);
        let mut env = mock_env();

        env.block.time = Timestamp::from_seconds(1571797719);
        let msg = ExecuteMsg::Transfer {
            recipient: addr2.clone(),
            amount: transfer,
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.messages.len(), 0);

        let remainder = amount1.checked_sub(transfer).unwrap();
        assert_eq!(get_balance(deps.as_ref(), addr1), remainder);
        assert_eq!(get_balance(deps.as_ref(), addr2), transfer);
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );
    }

    #[test]
    fn burn() {
        let mut deps = mock_dependencies_with_balance(&coins(2, "token"));
        let addr1 = String::from("addr0001");
        let amount1 = Uint128::from(12340000u128);
        let burn = Uint128::from(76543u128);
        let too_much = Uint128::from(12340321u128);

        do_instantiate(deps.as_mut(), &addr1, amount1);

        // cannot burn nothing
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Burn {
            amount: Uint128::zero(),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );

        // cannot burn more than we have
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Burn { amount: too_much };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );

        // valid burn reduces total supply
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Burn { amount: burn };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.messages.len(), 0);

        let remainder = amount1.checked_sub(burn).unwrap();
        assert_eq!(get_balance(deps.as_ref(), addr1), remainder);
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            remainder
        );
    }

    #[test]
    fn send() {
        let mut deps = mock_dependencies_with_balance(&coins(2, "token"));
        let addr1 = String::from("addr0001");
        let contract = String::from("addr0002");
        let amount1 = Uint128::from(12340000u128);
        let transfer = Uint128::from(76543u128);
        let too_much = Uint128::from(12340321u128);
        let send_msg = Binary::from(r#"{"some":123}"#.as_bytes());

        do_instantiate(deps.as_mut(), &addr1, amount1);

        // cannot send nothing
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Send {
            contract: contract.clone(),
            amount: Uint128::zero(),
            msg: send_msg.clone(),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});

        // cannot send more than we have
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Send {
            contract: contract.clone(),
            amount: too_much,
            msg: send_msg.clone(),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));

        // valid transfer
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Send {
            contract: contract.clone(),
            amount: transfer,
            msg: send_msg.clone(),
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.messages.len(), 1);

        // ensure proper send message sent
        // this is the message we want delivered to the other side
        let binary_msg = Cw20ReceiveMsg {
            sender: addr1.clone(),
            amount: transfer,
            msg: send_msg,
        }
        .into_binary()
        .unwrap();
        // and this is how it must be wrapped for the vm to process it
        assert_eq!(
            res.messages[0],
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract.clone(),
                msg: binary_msg,
                funds: vec![],
            }))
        );

        // ensure balance is properly transferred
        let remainder = amount1.checked_sub(transfer).unwrap();
        assert_eq!(get_balance(deps.as_ref(), addr1), remainder);
        assert_eq!(get_balance(deps.as_ref(), contract), transfer);
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );
    } 

    mod demurrage {
        use super::*;

        #[test]
        fn basic() {
            let mut deps = mock_dependencies();
            let amount = Uint128::from(11223344u128);
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![Cw20Coin {
                    address: String::from("addr0000"),
                    amount,
                }],
                mint: None,
                tax_level_minute: 50105908373373, // 20 decimals
                period_minutes: 1, 
                supply_cap: 10000000, //supply cap is 10 million coins
                sink_address: "sinkaddress".to_string()
            };

            

            let info = mock_info("creator", &[]);
            let env = mock_env();
            let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
            assert_eq!(0, res.messages.len());

            assert_eq!(
                query_token_info(deps.as_ref()).unwrap(),
                TokenInfoResponse {
                    name: "Cash Token".to_string(),
                    symbol: "CASH".to_string(),
                    decimals: 9,
                    total_supply: amount,
                }
            );
            assert_eq!(
                get_balance(deps.as_ref(), "addr0000"),
                Uint128::new(11223344)
            );
        }
    }
}