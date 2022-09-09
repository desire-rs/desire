use crate::model::{QueryUser, User};
use desire::{Request, Resp, Result};
use tracing::info;
pub async fn hello(req: Request) -> Result {
  let method = req.method().to_string();
  let msg = "hello world";
  println!("method: {}", method);
  Ok(Resp::data(msg).into())
}

pub async fn get_users(req: Request) -> Result {
  let method = req.method().to_string();
  println!("method: {}", method);
  Ok("get_users".into())
}
pub async fn get_user_by_id(req: Request) -> Result {
  let method = req.method().to_string();
  info!("method: {}", method);
  let id = req.get_param::<String>("id")?;
  Resp::data(id).into()
}

pub async fn get_query(req: Request) -> Result {
  let query = req.get_query::<QueryUser>()?;
  info!("query {:?}", query);
  Resp::data(query).into()
}
pub async fn create_users(req: Request) -> Result {
  let user = req.body::<User>().await?;
  info!("user: {:?}", user);
  Resp::data(user).into()
}
