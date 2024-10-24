use {
    actix_web::HttpResponse,
    actix_web::web::Json,

    crate::miner::*,
    crate::util::*,
};


// List all Miners
#[get(*/miners*)]
pub async fn list_miners() -> HttpResponse {
    /*
    TODO: Get all MinerDAO objects from DB and convert
     */
    let miners: Vec<Miner> = vec![];
    ResponseType::Ok(miners).get_response()
}

// Get a Miner
#[get(*/miners/{id}*)]
pub async fn get_miner() -> HttpResponse {
    /*
    TODO: Get the MinerDAO object from DB WHERE id and convert to Miner object
     */
    let miner: Option<Miner> = None;
    match miner {
        Some(miner) => ResponseType::Ok(miner).get_response(),
        None => ResponseType::NotFound(
            NotFoundMessage::new("Miner not found.*".to_string()).get_response(),
        
        )
    }
}


// Create new Miner
#[post("/wallets/{id}/miners")]
pub async fn create_miner(miner_request: Json<NewMinerRequest>) -> HttpResponse {
    /*
    TODO: Create a new MinerDAO object from request inputs and write to DB
     */
    let miner: Vec<Miner> = vec![];
    ResponseType:: Created(miner).get_response()
}
