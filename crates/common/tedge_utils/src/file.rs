use nix::unistd::*;
use std::fs::File;
use std::os::unix::fs::PermissionsExt;
use std::{fs, io};
use users::{get_group_by_name, get_user_by_name};

pub fn create_directory_with_user_group(
    grp_user: &str,
    dirs: Vec<&str>,
) -> Result<(), anyhow::Error> {
    for directory in dirs {
        match fs::create_dir(directory) {
            Ok(_) => {
                change_owner_and_permission(directory, grp_user, 0o775)?;
            }

            Err(e) => {
                if e.kind() == io::ErrorKind::AlreadyExists {
                    return Ok(());
                } else {
                    dbg!(
                        "failed to create the directory {} due to error {}",
                        directory, &e
                    );
                    return Err(e.into());
                }
            }
        }
    }
    Ok(())
}

pub fn create_file_with_user_group(grp_user: &str, files: Vec<&str>) -> Result<(), anyhow::Error> {
    for file in files {
        match File::create(file) {
            Ok(_) => {
                dbg!("created successfully");
                change_owner_and_permission(file, grp_user, 0o644)?;
            }
            Err(e) => {
                if e.kind() == io::ErrorKind::AlreadyExists {
                    return Ok(());
                } else {
                    dbg!("failed to create the file {} due to error {}", file, &e);
                    return Err(e.into());
                }
            }
        }
    }
    Ok(())
}

fn change_owner_and_permission(file: &str, grp_user: &str, mode: u32) -> anyhow::Result<()> {
    let ud = match get_user_by_name(grp_user) {
        Some(user) => user.uid(),
        None => {
            dbg!("user not found");
            anyhow::bail!("user not found");
        }
    };

    let gd = match get_group_by_name(grp_user) {
        Some(group) => group.gid(),
        None => {
            dbg!("grp not found");
            anyhow::bail!("group not found");
        }
    };

    chown(
        file,
        Some(Uid::from_raw(ud.into())),
        Some(Gid::from_raw(gd.into())),
    )?;

    let mut perm = fs::metadata(file)?.permissions();
    dbg!(&perm);
    perm.set_mode(mode);
    dbg!("after set mode");
    fs::set_permissions(file, perm)?;
    dbg!("after set perm");

    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    #[test]
    fn create_file()->anyhow::Result<()> {
        let user = whoami::username();
        let ruser = whoami::realname();
        dbg!(&user);
        dbg!(&ruser);
        let _ = create_file_with_user_group(&user, vec!["/home/runner/fcreate_test"])?;
        assert!(Path::new("/home/runner/fcreate_test").exists());
        let meta = std::fs::metadata("/home/runner/fcreate_test")?;
        let perm = meta.permissions();
        println!("{:o}", perm.mode());
        assert!(format!("{:o}", perm.mode()).contains("644"));
        Ok(())
    }

    #[test]
    fn create_directory()->anyhow::Result<()>  {
        let user = whoami::username();
        dbg!(&user);
        let _ = create_directory_with_user_group(&user, vec!["/tmp/fcreate_test_dir"])?;
        assert!(Path::new("/tmp/fcreate_test_dir").exists());
        let meta = std::fs::metadata("/tmp/fcreate_test_dir").unwrap();
        let perm = meta.permissions();
        println!("{:o}", perm.mode());
        assert!(format!("{:o}", perm.mode()).contains("775"));
        Ok(())
    }
}
