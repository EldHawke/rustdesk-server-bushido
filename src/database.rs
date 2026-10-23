use async_trait::async_trait;
use hbb_common::{log, ResultType};
use sqlx::{
    sqlite::SqliteConnectOptions, ConnectOptions, Connection, Error as SqlxError, SqliteConnection,
};
use std::{ops::DerefMut, str::FromStr};

type Pool = deadpool::managed::Pool<DbPool>;

pub const STATUS_WAITING: i64 = 0;
pub const STATUS_APPROVED: i64 = 1;
pub const STATUS_BLOCKED: i64 = 2;

pub struct DbPool {
    url: String,
}

#[async_trait]
impl deadpool::managed::Manager for DbPool {
    type Type = SqliteConnection;
    type Error = SqlxError;
    async fn create(&self) -> Result<SqliteConnection, SqlxError> {
        let mut opt = SqliteConnectOptions::from_str(&self.url).unwrap();
        opt.log_statements(log::LevelFilter::Debug);
        SqliteConnection::connect_with(&opt).await
    }
    async fn recycle(
        &self,
        obj: &mut SqliteConnection,
    ) -> deadpool::managed::RecycleResult<SqlxError> {
        Ok(obj.ping().await?)
    }
}

#[derive(Clone)]
pub struct Database {
    pool: Pool,
}

#[derive(sqlx::FromRow, Default)]
pub struct Peer {
    pub guid: Vec<u8>,
    pub id: String,
    pub uuid: Vec<u8>,
    pub pk: Vec<u8>,
    pub user: Option<Vec<u8>>,
    pub info: String,
    pub status: Option<i64>,
    pub note: Option<String>,
}

impl Database {
    pub async fn new(url: &str) -> ResultType<Database> {
        if !std::path::Path::new(url).exists() {
            std::fs::File::create(url).ok();
        }
        let n: usize = crate::common::get_arg_or("MAX_DATABASE_CONNECTIONS", "1".to_owned())
            .parse()
            .unwrap_or(1);
        log::debug!("MAX_DATABASE_CONNECTIONS={}", n);
        let pool = Pool::new(
            DbPool {
                url: url.to_owned(),
            },
            n,
        );
        let _ = pool.get().await?; // test
        let db = Database { pool };
        db.create_tables().await?;
        Ok(db)
    }

        async fn create_tables(&self) -> ResultType<()> {
			let mut conn = self.pool.get().await?;

			sqlx::query(
				"CREATE TABLE IF NOT EXISTS peer (
					guid blob primary key not null,
					id varchar(100) not null,
					uuid blob not null,
					pk blob not null,
					created_at datetime not null default(current_timestamp),
					user blob,
					status tinyint,
					note varchar(300),
					info text not null
				) WITHOUT ROWID",
			)
			.execute(conn.deref_mut())
			.await?;

			sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS index_peer_id ON peer (id)")
				.execute(conn.deref_mut())
				.await?;

			sqlx::query("CREATE INDEX IF NOT EXISTS index_peer_user ON peer (user)")
				.execute(conn.deref_mut())
				.await?;

			sqlx::query("CREATE INDEX IF NOT EXISTS index_peer_created_at ON peer (created_at)")
				.execute(conn.deref_mut())
				.await?;

			sqlx::query("CREATE INDEX IF NOT EXISTS index_peer_status ON peer (status)")
				.execute(conn.deref_mut())
				.await?;

			Ok(())
		}

    pub async fn get_peer(&self, id: &str) -> ResultType<Option<Peer>> {
        Ok(sqlx::query_as::<_, Peer>(
			"SELECT guid, id, uuid, pk, user, status, note, info FROM peer WHERE id = ?"
		)
		.bind(id)
		.fetch_optional(self.pool.get().await?.deref_mut())
		.await?)
    }
	
	pub async fn list_peers(
		&self,
		status: Option<i64>,
		page: u32,
		limit: u32,
	) -> ResultType<Vec<Peer>> {

		let offset = ((page.max(1) - 1) * limit.max(1)) as i64;
		let limit = limit.max(1) as i64;

		let peers = if let Some(status) = status {

			sqlx::query_as::<_, Peer>(
				r#"
				SELECT
					guid,
					id,
					uuid,
					pk,
					user,
					status,
					note,
					info
				FROM peer
				WHERE status = ?
				ORDER BY created_at DESC
				LIMIT ?
				OFFSET ?
				"#
			)
			.bind(status)
			.bind(limit)
			.bind(offset)
			.fetch_all(self.pool.get().await?.deref_mut())
			.await?

		} else {

			sqlx::query_as::<_, Peer>(
				r#"
				SELECT
					guid,
					id,
					uuid,
					pk,
					user,
					status,
					note,
					info
				FROM peer
				ORDER BY created_at DESC
				LIMIT ?
				OFFSET ?
				"#
			)
			.bind(limit)
			.bind(offset)
			.fetch_all(self.pool.get().await?.deref_mut())
			.await?

		};

		Ok(peers)
	}
	
	pub async fn count_peers(
		&self,
		status: Option<i64>,
	) -> ResultType<u64> {
		let count: i64 = if let Some(status) = status {
			sqlx::query_scalar("SELECT COUNT(*) FROM peer WHERE status = ?")
				.bind(status)
				.fetch_one(self.pool.get().await?.deref_mut())
				.await?
		} else {
			sqlx::query_scalar("SELECT COUNT(*) FROM peer")
				.fetch_one(self.pool.get().await?.deref_mut())
				.await?
		};

		Ok(count as u64)
	}
	
	pub async fn get_peer_summary(
		&self,
		id: &str,
	) -> ResultType<Option<Peer>> {

		Ok(
			sqlx::query_as::<_, Peer>(
				r#"
				SELECT
					guid,
					id,
					uuid,
					pk,
					user,
					status,
					note,
					info
				FROM peer
				WHERE id = ?
				"#
			)
			.bind(id)
			.fetch_optional(self.pool.get().await?.deref_mut())
			.await?
		)
	}

	pub async fn set_status(
		&self,
		id: &str,
		status: i64,
	) -> ResultType<()> {

        sqlx::query("UPDATE peer SET status = ? WHERE id = ?")
            .bind(status)
            .bind(id)
            .execute(self.pool.get().await?.deref_mut())
            .await?;

		Ok(())
	}

	pub async fn set_note(
			&self,
			id: &str,
			note: Option<&str>,
	) -> ResultType<()> {

        sqlx::query("UPDATE peer SET note = ? WHERE id = ?")
            .bind(note)
            .bind(id)
            .execute(self.pool.get().await?.deref_mut())
            .await?;

			Ok(())
	}

    pub async fn insert_peer(
        &self,
        id: &str,
        uuid: &[u8],
        pk: &[u8],
        info: &str,
    ) -> ResultType<Vec<u8>> {
        let guid = uuid::Uuid::new_v4().as_bytes().to_vec();
        sqlx::query("INSERT INTO peer(guid, id, uuid, pk, status, info) VALUES(?, ?, ?, ?, ?, ?)")
            .bind(&guid)
            .bind(id)
            .bind(uuid)
            .bind(pk)
            .bind(STATUS_WAITING)
            .bind(info)
            .execute(self.pool.get().await?.deref_mut())
            .await?;
        Ok(guid)
    }

    pub async fn update_pk(
        &self,
        guid: &Vec<u8>,
        id: &str,
        pk: &[u8],
        info: &str,
    ) -> ResultType<()> {
        sqlx::query("UPDATE peer SET id = ?, pk = ?, info = ? WHERE guid = ?")
            .bind(id)
            .bind(pk)
            .bind(info)
            .bind(guid)
            .execute(self.pool.get().await?.deref_mut())
            .await?;
        Ok(())
    }

    pub async fn get_status(&self, id: &str) -> ResultType<Option<i64>> {
		//log::warn!("DB get_status({})", id);
		let row = sqlx::query_as::<_, (Option<i64>,)>("SELECT status FROM peer WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool.get().await?.deref_mut())
            .await?;

        Ok(row.and_then(|r| r.0))
	}
}

#[cfg(test)]
mod tests {
    use hbb_common::tokio;
    #[test]
    fn test_insert() {
        insert();
    }

    #[tokio::main(flavor = "multi_thread")]
    async fn insert() {
        let db = super::Database::new("test.sqlite3").await.unwrap();
        let mut jobs = vec![];
        for i in 0..10000 {
            let cloned = db.clone();
            let id = i.to_string();
            let a = tokio::spawn(async move {
                let empty_vec = Vec::new();
                cloned
                    .insert_peer(&id, &empty_vec, &empty_vec, "")
                    .await
                    .unwrap();
            });
            jobs.push(a);
        }
        for i in 0..10000 {
            let cloned = db.clone();
            let id = i.to_string();
            let a = tokio::spawn(async move {
                cloned.get_peer(&id).await.unwrap();
            });
            jobs.push(a);
        }
        hbb_common::futures::future::join_all(jobs).await;
    }
}
