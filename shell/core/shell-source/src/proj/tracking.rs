use crate::{Observable, rx::Observable as _};
use futures_util::StreamExt;
use proj_model::{BUS_NAME, GoalTrackingInfo, MANAGER_INTERFACE, METRICS_INTERFACE, ROOT_PATH};

pub fn goal_tracking() -> Observable<Vec<GoalTrackingInfo>> {
    crate::shared_by_key("projd-tracking", "current", || {
        crate::from_task(|sender|async move {
        let result=async {
            let conn=zbus::Connection::session().await?;
            let proxy=zbus::Proxy::new(&conn,BUS_NAME,ROOT_PATH,METRICS_INTERFACE).await?;
            let manager=zbus::Proxy::new(&conn,BUS_NAME,ROOT_PATH,MANAGER_INTERFACE).await?;
            let bus=zbus::fdo::DBusProxy::new(&conn).await?;
            let mut owners=bus.receive_name_owner_changed().await?;
            let mut changes=manager.receive_signal("Changed").await?;
            loop {
                match proxy.call::<_,_,Vec<GoalTrackingInfo>>("ListGoalTracking",&()).await {
                    Ok(rows)=>if sender.send(Ok(rows)).await.is_err(){return Ok::<_,zbus::Error>(());},
                    Err(error)=>{
                        let transient=matches!(&error,zbus::Error::MethodError(name,_,_) if matches!(name.as_str(),"org.freedesktop.DBus.Error.ServiceUnknown"|"org.freedesktop.DBus.Error.NameHasNoOwner"|"org.freedesktop.DBus.Error.NoReply"));
                        if !transient {return Err(error);}
                        if sender.send(Ok(Vec::new())).await.is_err(){return Ok(());}
                    }
                }
                loop {
                    let refresh=tokio::select! {
                        event=changes.next()=>{if event.is_none(){return Ok(());} true},
                        event=owners.next()=>{
                            let Some(event)=event else{return Ok(());};
                            let args=event.args()?;
                            if args.name().as_str()!=BUS_NAME {false}
                            else if args.new_owner().is_some(){true}
                            else {
                                if sender.send(Ok(Vec::new())).await.is_err(){return Ok(());}
                                false
                            }
                        }
                    };
                    if refresh {break;}
                }
            }
        }.await;
        if let Err(error)=result {let _=sender.send(Err(error.to_string())).await;}
    }).distinct_until_changed().box_it()
    })
}
