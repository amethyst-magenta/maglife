use chrono::{DateTime, Local, NaiveDate, NaiveTime, TimeZone, Utc};

pub fn local_instant(day: NaiveDate, time: NaiveTime) -> Result<DateTime<Utc>, &'static str> {
    Local.from_local_datetime(&day.and_time(time)).single().map(|value| value.with_timezone(&Utc))
        .ok_or("Местное время неоднозначно или отсутствует из-за смены часового пояса. Выберите другое время.")
}

pub fn day_bounds(day: NaiveDate) -> Result<(DateTime<Utc>, DateTime<Utc>), &'static str> {
    let next = day
        .succ_opt()
        .ok_or("Дата выходит за допустимые границы.")?;
    Ok((
        local_instant(day, NaiveTime::MIN)?,
        local_instant(next, NaiveTime::MIN)?,
    ))
}
