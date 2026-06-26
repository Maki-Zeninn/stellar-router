#![no_std]

//! # router-quote
//!
//! Quote calculation and route comparison for the stellar-router suite.
//! Provides configurable fee-based quote calculations and best-route selection.
//!
//! ## Features
//! - Configurable fee basis points (fee_bps) per route
//! - Multiple quote comparison
//! - Best quote selection based on highest output amount
//! - Integration with liquidity plugins

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, String, Symbol, Vec,
};

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Admin,
    /// Route name -> fee in basis points (1 bps = 0.01%)
    RouteFee(String),
    /// Default fee if route-specific fee not set (in basis points)
    DefaultFee,
    ConfiguredRoutes,
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct QuoteRequest {
    /// Route name to get quote for
    pub route: String,
    /// Input token address
    pub token_in: Address,
    /// Output token address
    pub token_out: Address,
    /// Amount of input token
    pub amount_in: i128,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct QuoteResponse {
    /// Route name
    pub route: String,
    /// Input token address
    pub token_in: Address,
    /// Output token address
    pub token_out: Address,
    /// Amount of input token
    pub amount_in: i128,
    /// Expected output amount after fees
    pub amount_out: i128,
    /// Fee amount deducted (in input token units)
    pub fee_amount: i128,
    /// Fee in basis points used for this quote
    pub fee_bps: u32,
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum QuoteError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidAmount = 4,
    InvalidFeeBps = 5,
    NoQuotesProvided = 6,
    RouteNotFound = 7,
    ArithmeticOverflow = 8,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct RouterQuote;

#[contractimpl]
impl RouterQuote {
    /// Initialize the quote contract with an admin address and default fee.
    ///
    /// Sets up the admin and default fee in basis points (bps).
    /// 1 bps = 0.01%, so 100 bps = 1%.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `admin` - The address that will have admin privileges.
    /// * `default_fee_bps` - Default fee in basis points (max 10000 = 100%).
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`QuoteError::AlreadyInitialized`] — if already initialized.
    /// * [`QuoteError::InvalidFeeBps`] — if fee_bps > 10000.
    pub fn initialize(env: Env, admin: Address, default_fee_bps: u32) -> Result<(), QuoteError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(QuoteError::AlreadyInitialized);
        }

        if default_fee_bps > 10000 {
            return Err(QuoteError::InvalidFeeBps);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::DefaultFee, &default_fee_bps);

        env.events().publish(
            (Symbol::new(&env, "initialized"),),
            (admin, default_fee_bps),
        );

        Ok(())
    }

    /// Set fee in basis points for a specific route.
    ///
    /// Allows admin to configure per-route fees. If not set, the default fee
    /// is used.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be the admin.
    /// * `route` - The route name to configure.
    /// * `fee_bps` - Fee in basis points (max 10000 = 100%).
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`QuoteError::Unauthorized`] — if caller is not the admin.
    /// * [`QuoteError::InvalidFeeBps`] — if fee_bps > 10000.
    pub fn set_route_fee(
        env: Env,
        caller: Address,
        route: String,
        fee_bps: u32,
    ) -> Result<(), QuoteError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        if fee_bps > 10000 {
            return Err(QuoteError::InvalidFeeBps);
        }

        // Maintain route index — only append if not already tracked
        let mut routes = Self::read_configured_routes(&env);
        if !routes.contains(&route) {
            routes.push_back(route.clone());
            Self::write_configured_routes(&env, &routes);
        }

        env.storage()
            .instance()
            .set(&DataKey::RouteFee(route.clone()), &fee_bps);

        env.events()
            .publish((Symbol::new(&env, "route_fee_set"),), (route, fee_bps));

        Ok(())
    }

    /// Get fee in basis points for a specific route.
    ///
    /// Returns the route-specific fee if set, otherwise returns the default fee.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `route` - The route name.
    ///
    /// # Returns
    /// Fee in basis points.
    pub fn get_route_fee(env: Env, route: String) -> u32 {
        env.storage()
            .instance()
            .get::<DataKey, u32>(&DataKey::RouteFee(route))
            .unwrap_or_else(|| {
                env.storage()
                    .instance()
                    .get(&DataKey::DefaultFee)
                    .unwrap_or(100) // Default to 1% if not initialized
            })
    }

    /// Get all configured router fee.
    ///
    /// Returns a vector of route_name and fee_bps.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Returns
    /// Router name and Fee in basis points.
    pub fn get_all_configured_routes(env: Env) -> Vec<(String, u32)> {
        let routes = Self::read_configured_routes(&env);
        let mut configures_routes = Vec::new(&env);

        for route in routes {
            let fee = Self::get_route_fee(env.clone(), route.clone());
            configures_routes.push_back((route, fee));
        }
        configures_routes
    }

    /// Get a quote for a single route with configurable fee.
    ///
    /// Calculates the expected output amount after deducting fees based on
    /// the route's configured fee_bps.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `request` - Quote request containing route, tokens, and amount.
    ///
    /// # Returns
    /// [`QuoteResponse`] with calculated amounts and fees.
    ///
    /// # Errors
    /// * [`QuoteError::InvalidAmount`] — if amount_in <= 0.
    /// * [`QuoteError::ArithmeticOverflow`] — if the fee or output calculation overflows.
    pub fn get_quote(env: Env, request: QuoteRequest) -> Result<QuoteResponse, QuoteError> {
        if request.amount_in <= 0 {
            return Err(QuoteError::InvalidAmount);
        }

        let fee_bps = Self::get_route_fee(env.clone(), request.route.clone());

        // Calculate fee: fee_amount = amount_in * fee_bps / 10000
        let fee_amount = request
            .amount_in
            .checked_mul(fee_bps as i128)
            .and_then(|v| v.checked_div(10000))
            .ok_or(QuoteError::ArithmeticOverflow)?;

        // Calculate output: amount_out = amount_in - fee_amount
        let amount_out = request
            .amount_in
            .checked_sub(fee_amount)
            .ok_or(QuoteError::ArithmeticOverflow)?;

        let response = QuoteResponse {
            route: request.route.clone(),
            token_in: request.token_in,
            token_out: request.token_out,
            amount_in: request.amount_in,
            amount_out,
            fee_amount,
            fee_bps,
        };

        env.events().publish(
            (Symbol::new(&env, "quote_calculated"),),
            (request.route, amount_out, fee_amount),
        );

        Ok(response)
    }

    /// Get quotes for multiple routes.
    ///
    /// Calculates quotes for all provided requests and returns them as a vector.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `requests` - Vector of quote requests.
    ///
    /// # Returns
    /// Vector of [`QuoteResponse`] for each request.
    ///
    /// # Errors
    /// * [`QuoteError::NoQuotesProvided`] — if requests vector is empty.
    /// * [`QuoteError::InvalidAmount`] — if any amount_in <= 0.
    pub fn get_quotes(
        env: Env,
        requests: Vec<QuoteRequest>,
    ) -> Result<Vec<QuoteResponse>, QuoteError> {
        if requests.is_empty() {
            return Err(QuoteError::NoQuotesProvided);
        }

        let mut responses = Vec::new(&env);

        for request in requests.iter() {
            let response = Self::get_quote(env.clone(), request)?;
            responses.push_back(response);
        }

        Ok(responses)
    }

    /// Get the best quote from multiple routes.
    ///
    /// Calculates quotes for all provided requests and returns the single
    /// quote with the highest amount_out. Useful for automatic route selection.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `requests` - Vector of quote requests to compare.
    ///
    /// # Returns
    /// The [`QuoteResponse`] with the highest amount_out.
    ///
    /// # Errors
    /// * [`QuoteError::NoQuotesProvided`] — if requests vector is empty.
    /// * [`QuoteError::InvalidAmount`] — if any amount_in <= 0.
    pub fn get_best_quote(
        env: Env,
        requests: Vec<QuoteRequest>,
    ) -> Result<QuoteResponse, QuoteError> {
        let quotes = Self::get_quotes(env.clone(), requests)?;

        let mut best_quote = quotes.get(0).unwrap();

        for i in 1..quotes.len() {
            let quote = quotes.get(i).unwrap();
            if quote.amount_out > best_quote.amount_out {
                best_quote = quote;
            }
        }

        env.events().publish(
            (Symbol::new(&env, "best_quote_selected"),),
            (best_quote.route.clone(), best_quote.amount_out),
        );

        Ok(best_quote)
    }

    /// Get all valid quotes sorted by amount_out descending.
    ///
    /// Calculates quotes for all provided requests, silently omitting any
    /// that fail (e.g. invalid amount, arithmetic overflow), and returns the
    /// remaining quotes sorted from highest to lowest `amount_out`.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `requests` - Vector of quote requests to evaluate.
    ///
    /// # Returns
    /// Sorted [`Vec<QuoteResponse>`] (highest `amount_out` first). Returns an
    /// empty vec if all requests fail.
    ///
    /// # Errors
    /// * [`QuoteError::NoQuotesProvided`] — if requests vector is empty.
    pub fn get_quotes_sorted(
        env: Env,
        requests: Vec<QuoteRequest>,
    ) -> Result<Vec<QuoteResponse>, QuoteError> {
        if requests.is_empty() {
            return Err(QuoteError::NoQuotesProvided);
        }

        // Collect valid quotes, skipping failures.
        let mut sorted: Vec<QuoteResponse> = Vec::new(&env);
        for request in requests.iter() {
            if let Ok(response) = Self::get_quote(env.clone(), request) {
                // Insertion sort: find position where response.amount_out fits.
                let mut pos = sorted.len();
                for i in 0..sorted.len() {
                    if response.amount_out > sorted.get(i).unwrap().amount_out {
                        pos = i;
                        break;
                    }
                }
                sorted.insert(pos, response);
            }
        }

        Ok(sorted)
    }

    /// Update the default fee in basis points.
    ///
    /// Changes the default fee used when a route-specific fee is not set.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be the admin.
    /// * `fee_bps` - New default fee in basis points (max 10000 = 100%).
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`QuoteError::Unauthorized`] — if caller is not the admin.
    /// * [`QuoteError::InvalidFeeBps`] — if fee_bps > 10000.
    pub fn set_default_fee(env: Env, caller: Address, fee_bps: u32) -> Result<(), QuoteError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        if fee_bps > 10000 {
            return Err(QuoteError::InvalidFeeBps);
        }

        env.storage().instance().set(&DataKey::DefaultFee, &fee_bps);

        env.events()
            .publish((Symbol::new(&env, "default_fee_updated"),), fee_bps);

        Ok(())
    }

    /// Get the current default fee in basis points.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Returns
    /// Default fee in basis points.
    pub fn get_default_fee(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::DefaultFee)
            .unwrap_or(100) // Default to 1% if not initialized
    }

    /// Get current admin address.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Returns
    /// The admin [`Address`].
    ///
    /// # Errors
    /// * [`QuoteError::NotInitialized`] — if the contract has not been initialized.
    pub fn admin(env: Env) -> Result<Address, QuoteError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(QuoteError::NotInitialized)
    }

    /// Transfer admin to a new address.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `current` - The current admin address; must authenticate.
    /// * `new_admin` - The address that will become the new admin.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`QuoteError::Unauthorized`] — if current is not the admin.
    pub fn transfer_admin(
        env: Env,
        current: Address,
        new_admin: Address,
    ) -> Result<(), QuoteError> {
        current.require_auth();
        Self::require_admin(&env, &current)?;
        env.storage().instance().set(&DataKey::Admin, &new_admin);

        env.events().publish(
            (Symbol::new(&env, "admin_transferred"),),
            (current, new_admin),
        );

        Ok(())
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn require_admin(env: &Env, caller: &Address) -> Result<(), QuoteError> {
        let admin = Self::admin(env.clone())?;
        if &admin != caller {
            return Err(QuoteError::Unauthorized);
        }
        Ok(())
    }

    fn read_configured_routes(env: &Env) -> Vec<String> {
        env.storage()
            .instance()
            .get(&DataKey::ConfiguredRoutes)
            .unwrap_or_else(|| Vec::new(env))
    }

    fn write_configured_routes(env: &Env, routes: &Vec<String>) {
        env.storage()
            .instance()
            .set(&DataKey::ConfiguredRoutes, routes);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        vec, Env, String,
    };

    fn setup() -> (Env, Address, RouterQuoteClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterQuote);
        let client = RouterQuoteClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin, &100); // 1% default fee
        (env, admin, client)
    }

    #[test]
    fn test_initialize() {
        let (env, admin, client) = setup();
        assert_eq!(client.admin(), admin);
        assert_eq!(client.get_default_fee(), 100);
    }

    #[test]
    fn test_initialize_twice_fails() {
        let (env, admin, client) = setup();
        let result = client.try_initialize(&admin, &100);
        assert_eq!(result, Err(Ok(QuoteError::AlreadyInitialized)));
    }

    #[test]
    fn test_initialize_invalid_fee_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterQuote);
        let client = RouterQuoteClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let result = client.try_initialize(&admin, &10001);
        assert_eq!(result, Err(Ok(QuoteError::InvalidFeeBps)));
    }

    #[test]
    fn test_set_and_get_route_fee() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "uniswap");
        client.set_route_fee(&admin, &route, &50); // 0.5%
        assert_eq!(client.get_route_fee(&route), 50);
    }

    #[test]
    fn test_get_route_fee_returns_default_when_not_set() {
        let (env, _admin, client) = setup();
        let route = String::from_str(&env, "uniswap");
        assert_eq!(client.get_route_fee(&route), 100); // Default 1%
    }

    #[test]
    fn test_get_quote_with_default_fee() {
        let (env, _admin, client) = setup();
        let token_in = Address::generate(&env);
        let token_out = Address::generate(&env);
        let request = QuoteRequest {
            route: String::from_str(&env, "uniswap"),
            token_in: token_in.clone(),
            token_out: token_out.clone(),
            amount_in: 10000,
        };

        let response = client.get_quote(&request);
        assert_eq!(response.amount_in, 10000);
        assert_eq!(response.fee_bps, 100); // 1%
        assert_eq!(response.fee_amount, 100); // 10000 * 100 / 10000 = 100
        assert_eq!(response.amount_out, 9900); // 10000 - 100 = 9900
    }

    #[test]
    fn test_get_quote_with_custom_route_fee() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "sushiswap");
        client.set_route_fee(&admin, &route, &30); // 0.3%

        let token_in = Address::generate(&env);
        let token_out = Address::generate(&env);
        let request = QuoteRequest {
            route: route.clone(),
            token_in: token_in.clone(),
            token_out: token_out.clone(),
            amount_in: 10000,
        };

        let response = client.get_quote(&request);
        assert_eq!(response.fee_bps, 30); // 0.3%
        assert_eq!(response.fee_amount, 30); // 10000 * 30 / 10000 = 30
        assert_eq!(response.amount_out, 9970); // 10000 - 30 = 9970
    }

    #[test]
    fn test_get_quote_invalid_amount_fails() {
        let (env, _admin, client) = setup();
        let token_in = Address::generate(&env);
        let token_out = Address::generate(&env);
        let request = QuoteRequest {
            route: String::from_str(&env, "uniswap"),
            token_in,
            token_out,
            amount_in: 0,
        };

        let result = client.try_get_quote(&request);
        assert_eq!(result, Err(Ok(QuoteError::InvalidAmount)));
    }

    #[test]
    fn test_get_quote_negative_amount_fails() {
        let (env, _admin, client) = setup();
        let token_in = Address::generate(&env);
        let token_out = Address::generate(&env);
        let request = QuoteRequest {
            route: String::from_str(&env, "uniswap"),
            token_in,
            token_out,
            amount_in: -5000,
        };

        let result = client.try_get_quote(&request);
        assert_eq!(result, Err(Ok(QuoteError::InvalidAmount)));
    }

    #[test]
    fn test_get_quote_max_amount_in_overflows() {
        let (env, _admin, client) = setup(); // default fee_bps = 100
        let token_in = Address::generate(&env);
        let token_out = Address::generate(&env);
        let request = QuoteRequest {
            route: String::from_str(&env, "uniswap"),
            token_in,
            token_out,
            amount_in: i128::MAX,
        };

        // amount_in * fee_bps overflows i128 before the division by 10000 can
        // bring it back down, so the quote must fail rather than silently
        // falling back to a zero fee (which would bypass the fee entirely).
        let result = client.try_get_quote(&request);
        assert_eq!(result, Err(Ok(QuoteError::ArithmeticOverflow)));
    }

    #[test]
    fn test_get_quote_rounds_fee_down_to_zero() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "uniswap");
        client.set_route_fee(&admin, &route, &1); // 0.01%

        let token_in = Address::generate(&env);
        let token_out = Address::generate(&env);
        let request = QuoteRequest {
            route,
            token_in,
            token_out,
            amount_in: 1,
        };

        let response = client.get_quote(&request);
        // 1 * 1 / 10000 truncates to 0: the fee rounds down and the full
        // amount passes through.
        assert_eq!(response.fee_amount, 0);
        assert_eq!(response.amount_out, 1);
    }

    #[test]
    fn test_get_quote_full_fee_takes_entire_amount() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "uniswap");
        client.set_route_fee(&admin, &route, &10000); // 100%

        let token_in = Address::generate(&env);
        let token_out = Address::generate(&env);
        let request = QuoteRequest {
            route,
            token_in,
            token_out,
            amount_in: 10000,
        };

        let response = client.get_quote(&request);
        assert_eq!(response.fee_amount, 10000);
        assert_eq!(response.amount_out, 0);
    }

    #[test]
    fn test_get_quote_near_full_fee() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "uniswap");
        client.set_route_fee(&admin, &route, &9999); // 99.99%

        let token_in = Address::generate(&env);
        let token_out = Address::generate(&env);
        let request = QuoteRequest {
            route,
            token_in,
            token_out,
            amount_in: 10000,
        };

        let response = client.get_quote(&request);
        assert_eq!(response.fee_amount, 9999);
        assert_eq!(response.amount_out, 10000 / 10000);
    }

    #[test]
    fn test_get_quotes_fails_entirely_when_one_request_overflows() {
        let (env, _admin, client) = setup();
        let token_in = Address::generate(&env);
        let token_out = Address::generate(&env);

        let mut requests = Vec::new(&env);
        requests.push_back(QuoteRequest {
            route: String::from_str(&env, "uniswap"),
            token_in: token_in.clone(),
            token_out: token_out.clone(),
            amount_in: 10000,
        });
        requests.push_back(QuoteRequest {
            route: String::from_str(&env, "sushiswap"),
            token_in,
            token_out,
            amount_in: i128::MAX,
        });

        // A batch must not silently return partial/incorrect results when one
        // quote in it overflows — the whole call should fail.
        let result = client.try_get_quotes(&requests);
        assert_eq!(result, Err(Ok(QuoteError::ArithmeticOverflow)));
    }

    #[test]
    fn test_get_quotes_multiple_routes() {
        let (env, admin, client) = setup();

        // Set different fees for different routes
        let route1 = String::from_str(&env, "uniswap");
        let route2 = String::from_str(&env, "sushiswap");
        client.set_route_fee(&admin, &route1, &100); // 1%
        client.set_route_fee(&admin, &route2, &30); // 0.3%

        let token_in = Address::generate(&env);
        let token_out = Address::generate(&env);

        let mut requests = Vec::new(&env);
        requests.push_back(QuoteRequest {
            route: route1.clone(),
            token_in: token_in.clone(),
            token_out: token_out.clone(),
            amount_in: 10000,
        });
        requests.push_back(QuoteRequest {
            route: route2.clone(),
            token_in: token_in.clone(),
            token_out: token_out.clone(),
            amount_in: 10000,
        });

        let responses = client.get_quotes(&requests);
        assert_eq!(responses.len(), 2);

        let resp1 = responses.get(0).unwrap();
        assert_eq!(resp1.route, route1);
        assert_eq!(resp1.amount_out, 9900); // 1% fee

        let resp2 = responses.get(1).unwrap();
        assert_eq!(resp2.route, route2);
        assert_eq!(resp2.amount_out, 9970); // 0.3% fee
    }

    #[test]
    fn test_get_quotes_empty_fails() {
        let (env, _admin, client) = setup();
        let requests = Vec::new(&env);
        let result = client.try_get_quotes(&requests);
        assert_eq!(result, Err(Ok(QuoteError::NoQuotesProvided)));
    }

    #[test]
    fn test_get_best_quote() {
        let (env, admin, client) = setup();

        // Set different fees for different routes
        let route1 = String::from_str(&env, "uniswap");
        let route2 = String::from_str(&env, "sushiswap");
        let route3 = String::from_str(&env, "pancakeswap");
        client.set_route_fee(&admin, &route1, &100); // 1%
        client.set_route_fee(&admin, &route2, &30); // 0.3% - best
        client.set_route_fee(&admin, &route3, &50); // 0.5%

        let token_in = Address::generate(&env);
        let token_out = Address::generate(&env);

        let mut requests = Vec::new(&env);
        requests.push_back(QuoteRequest {
            route: route1.clone(),
            token_in: token_in.clone(),
            token_out: token_out.clone(),
            amount_in: 10000,
        });
        requests.push_back(QuoteRequest {
            route: route2.clone(),
            token_in: token_in.clone(),
            token_out: token_out.clone(),
            amount_in: 10000,
        });
        requests.push_back(QuoteRequest {
            route: route3.clone(),
            token_in: token_in.clone(),
            token_out: token_out.clone(),
            amount_in: 10000,
        });

        let best = client.get_best_quote(&requests);
        assert_eq!(best.route, route2); // sushiswap has lowest fee
        assert_eq!(best.amount_out, 9970); // Best output
        assert_eq!(best.fee_bps, 30);
    }

    #[test]
    fn test_get_best_quote_empty_fails() {
        let (env, _admin, client) = setup();
        let requests = Vec::new(&env);
        let result = client.try_get_best_quote(&requests);
        assert_eq!(result, Err(Ok(QuoteError::NoQuotesProvided)));
    }

    #[test]
    fn test_set_default_fee() {
        let (env, admin, client) = setup();
        client.set_default_fee(&admin, &200); // 2%
        assert_eq!(client.get_default_fee(), 200);
    }

    #[test]
    fn test_set_default_fee_invalid_fails() {
        let (env, admin, client) = setup();
        let result = client.try_set_default_fee(&admin, &10001);
        assert_eq!(result, Err(Ok(QuoteError::InvalidFeeBps)));
    }

    #[test]
    fn test_transfer_admin() {
        let (env, admin, client) = setup();
        let new_admin = Address::generate(&env);
        client.transfer_admin(&admin, &new_admin);
        assert_eq!(client.admin(), new_admin);
    }

    #[test]
    fn test_unauthorized_set_route_fee_fails() {
        let (env, _admin, client) = setup();
        let unauthorized = Address::generate(&env);
        let route = String::from_str(&env, "uniswap");
        let result = client.try_set_route_fee(&unauthorized, &route, &50);
        assert_eq!(result, Err(Ok(QuoteError::Unauthorized)));
    }

    #[test]
    fn test_admin_getter() {
        let (env, admin, client) = setup();
        assert_eq!(client.admin(), admin);
    }

    #[test]
    fn test_get_all_configured_routes() {
        let (env, admin, client) = setup();
        let uniswap = String::from_str(&env, "uniswap");
        client.set_route_fee(&admin, &uniswap, &50); // 0.5%
        let sushiswap = String::from_str(&env, "sushiswap");
        client.set_route_fee(&admin, &sushiswap, &10); // 0.1%
        let balancer = String::from_str(&env, "balancer");
        client.set_route_fee(&admin, &balancer, &40); // 0.4%
        let aerodrome = String::from_str(&env, "aerodrome");
        client.set_route_fee(&admin, &aerodrome, &50); // 0.5%

        let all_configured_routes = client.get_all_configured_routes();
        assert_eq!(all_configured_routes.len(), 4);
        assert_eq!(
            all_configured_routes.get(0).unwrap().0,
            String::from_str(&env, "uniswap")
        );
        assert_eq!(all_configured_routes.get(0).unwrap().1, 50);
        assert_eq!(
            all_configured_routes.get(1).unwrap().0,
            String::from_str(&env, "sushiswap")
        );
        assert_eq!(all_configured_routes.get(1).unwrap().1, 10);
        assert_eq!(
            all_configured_routes.get(2).unwrap().0,
            String::from_str(&env, "balancer")
        );
        assert_eq!(all_configured_routes.get(2).unwrap().1, 40);
        assert_eq!(
            all_configured_routes.get(3).unwrap().0,
            String::from_str(&env, "aerodrome")
        );
        assert_eq!(all_configured_routes.get(3).unwrap().1, 50);
    }

    #[test]
    fn test_get_quotes_sorted_returns_descending_order() {
        let (env, admin, client) = setup();

        let route1 = String::from_str(&env, "uniswap");
        let route2 = String::from_str(&env, "sushiswap");
        let route3 = String::from_str(&env, "pancakeswap");
        client.set_route_fee(&admin, &route1, &100); // 1%   -> out 9900
        client.set_route_fee(&admin, &route2, &30);  // 0.3% -> out 9970 (best)
        client.set_route_fee(&admin, &route3, &50);  // 0.5% -> out 9950

        let token_in = Address::generate(&env);
        let token_out = Address::generate(&env);
        let mut requests = Vec::new(&env);
        for route in [&route1, &route2, &route3] {
            requests.push_back(QuoteRequest {
                route: route.clone(),
                token_in: token_in.clone(),
                token_out: token_out.clone(),
                amount_in: 10000,
            });
        }

        let sorted = client.get_quotes_sorted(&requests);
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted.get(0).unwrap().amount_out, 9970); // sushiswap
        assert_eq!(sorted.get(1).unwrap().amount_out, 9950); // pancakeswap
        assert_eq!(sorted.get(2).unwrap().amount_out, 9900); // uniswap
    }

    #[test]
    fn test_get_quotes_sorted_skips_invalid_quotes() {
        let (env, _admin, client) = setup();

        let token_in = Address::generate(&env);
        let token_out = Address::generate(&env);
        let mut requests = Vec::new(&env);
        // Valid request
        requests.push_back(QuoteRequest {
            route: String::from_str(&env, "uniswap"),
            token_in: token_in.clone(),
            token_out: token_out.clone(),
            amount_in: 10000,
        });
        // Invalid request (amount_in <= 0) — should be skipped
        requests.push_back(QuoteRequest {
            route: String::from_str(&env, "sushiswap"),
            token_in: token_in.clone(),
            token_out: token_out.clone(),
            amount_in: 0,
        });
        // Another invalid request (overflow) — should be skipped
        requests.push_back(QuoteRequest {
            route: String::from_str(&env, "pancakeswap"),
            token_in,
            token_out,
            amount_in: i128::MAX,
        });

        let sorted = client.get_quotes_sorted(&requests);
        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted.get(0).unwrap().route, String::from_str(&env, "uniswap"));
    }

    #[test]
    fn test_get_quotes_sorted_empty_fails() {
        let (env, _admin, client) = setup();
        let requests = Vec::new(&env);
        let result = client.try_get_quotes_sorted(&requests);
        assert_eq!(result, Err(Ok(QuoteError::NoQuotesProvided)));
    }

    #[test]
    fn test_get_quotes_sorted_all_invalid_returns_empty() {
        let (env, _admin, client) = setup();
        let token_in = Address::generate(&env);
        let token_out = Address::generate(&env);
        let mut requests = Vec::new(&env);
        requests.push_back(QuoteRequest {
            route: String::from_str(&env, "uniswap"),
            token_in,
            token_out,
            amount_in: -1,
        });

        let sorted = client.get_quotes_sorted(&requests);
        assert_eq!(sorted.len(), 0);
    }
}
