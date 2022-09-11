use crate::model::{QueryUser, User};
use crate::types::{ApiResult, Resp};
use desire::Request;
use tracing::info;
pub async fn hello(req: Request) -> ApiResult<String> {
  let method = req.method().to_string();
  let msg = "hello world".to_string();
  let remote = req.remote_addr;
  println!("method: {} {:?}", method, remote);
  Ok(Resp::data(msg))
}

pub async fn error(req: Request) -> ApiResult<String> {
  let method = req.method().to_string();
  let msg = "hello world".to_string();
  let remote = req.remote_addr;
  println!("method: {} {:?}", method, remote);
  Ok(Resp::data(msg))
}

pub async fn get_users(req: Request) -> ApiResult<String> {
  let method = req.method().to_string();
  println!("method: {}", method);
  Ok(Resp::data(method))
}
pub async fn get_user_by_id(req: Request) -> ApiResult<String> {
  let method = req.method().to_string();
  info!("method: {}", method);
  let id = req.get_param::<String>("id")?;
  Ok(Resp::data(id))
}

pub async fn get_query(req: Request) -> ApiResult<Option<QueryUser>> {
  let query = req.get_query::<QueryUser>()?;
  info!("query {:?}", query);
  Ok(Resp::data(query))
}
pub async fn create_users(req: Request) -> ApiResult<User> {
  let user = req.body::<User>().await?;
  info!("user: {:?}", user);
  Ok(Resp::data(user))
}
