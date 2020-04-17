use std::time::{Duration, Instant};

use actix::prelude::*;
use actix_web::{middleware, web, App, Error, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;

/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    // Create Http server with websocket support
    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .service(web::resource("/ws/").route(web::get().to(ws_index)))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

async fn ws_index(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    ws::start(WebSocket::new(), &req, stream)
}

struct WebSocket {
    last_heartbeat: Instant,
}

impl Actor for WebSocket {
    type Context = ws::WebsocketContext<Self>;

    /// Method is called on actor start. We start the heartbeat process here.
    fn started(&mut self, ctx: &mut Self::Context) {
        self.send_heartbeat(ctx);
        self.hello_world(ctx);
    }
}

/// Handler for `ws::Message`
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebSocket {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        // process websocket messages
        log::debug!("WS handle: {:?}", msg);
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.last_heartbeat = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.last_heartbeat = Instant::now();
            }
            Ok(ws::Message::Text(text)) => ctx.text(text),
            Ok(ws::Message::Binary(bin)) => ctx.binary(bin),
            Ok(ws::Message::Close(_)) => {
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}

impl WebSocket {
    fn new() -> Self {
        Self { last_heartbeat: Instant::now() }
    }

    /// Send a heartbeat to the client.
    fn send_heartbeat(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.last_heartbeat) > CLIENT_TIMEOUT {
                // Heartbeat timed out
                log::info!("Websocket client heartbeat failed, disconnecting!");
                ctx.stop();
                return;
            }

            ctx.ping(b"");
        });
    }

    /// Send hello world data to client for testing.
    fn hello_world(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(Duration::from_secs(3), |_act, ctx| {
            log::info!("Sending hello world");
            let msg = b"Hello World".to_vec();
            ctx.binary(msg);
        });
    }
}
