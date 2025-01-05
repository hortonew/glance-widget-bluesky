use actix_web::{get, http::header, web, App, HttpResponse, HttpServer, Responder};
use std::collections::HashMap;

#[get("/")]
async fn index(query: web::Query<HashMap<String, String>>) -> impl Responder {
    // The existing HTML content
    let main_body = r#"
        <p class="size-h1">Parameters:</p>
    "#;

    // Start with the main body content
    let mut body = String::from(main_body);

    // Append query parameters to the body
    // e.g., for /?name=John&city=London, you’ll see:
    // <p><strong>name</strong>: John</p>
    // <p><strong>city</strong>: London</p>
    for (key, value) in query.iter() {
        body.push_str(&format!("<p><strong>{}</strong>: {}</p>", key, value));
    }

    HttpResponse::Ok()
        .insert_header(("Widget-Title", "Test"))
        .insert_header(("Widget-Content-Type", "html"))
        .insert_header(header::ContentType::html())
        .body(body)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(index))
        .bind(("0.0.0.0", 8080))?
        .run()
        .await
}
