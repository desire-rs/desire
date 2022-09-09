use crate::model::{QueryUser, User};
use desire::{Context, Request, Resp, Result};
use tracing::info;
pub async fn hello(req: Request, _ctx: Context) -> Result {
  let method = req.method().to_string();
  let msg = "hello world";
  println!("method: {}", method);
  Ok(Resp::data(msg).into())
}

pub async fn get_users(req: Request, _ctx: Context) -> Result {
  let method = req.method().to_string();
  println!("method: {}", method);
  Ok("get_users".into())
}
pub async fn get_user_by_id(req: Request, ctx: Context) -> Result {
  let method = req.method().to_string();
  let params = ctx.params;
  info!("method: {}", method);
  info!("params: {:?}", params);
  Ok("get_users".into())
}

pub async fn get_query(req: Request, _ctx: Context) -> Result {
  let query = req.get_query::<QueryUser>()?;
  info!("query {:?}", query);
  Ok(Resp::data(query).into())
}
pub async fn create_users(req: Request, _ctx: Context) -> Result {
  let user = req.body::<User>().await?;
  info!("user: {:?}", user);
  Ok("create_users".into())
}
