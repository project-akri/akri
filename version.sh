#!/usr/bin/env bash

check_file_version()
{
    FILE=$1
    LINE_PATTERN=$2
    VERSION_STRING=$3

    echo "Check $FILE"
    
    CORRECT_VERSION=$(cat $FILE | grep "$LINE_PATTERN" | grep "$VERSION_STRING")
    if [ "$CORRECT_VERSION" == "" ]; then
    echo "    Needs upate: $FILE"
    echo "        To ensure all files match version.txt, run: version.sh -u -s"
    return 1
    else
    echo "    Verified update: $FILE ($CORRECT_VERSION)"
    fi
    return 0
}

check_twoline_version()
{
    FILE=$1
    LINE1_PATTERN=$2
    VERSION_STRING=$3

    echo "Check $FILE ($LINE1_PATTERN :: $VERSION_STRING)"
    
    CORRECT_VERSION=$(grep "$LINE1_PATTERN" -A 1 $FILE | grep "$VERSION_STRING")
    if [ "$CORRECT_VERSION" == "" ]; then
    echo "    Needs upate: $FILE"
    echo "        To ensure all files match version.txt, run: version.sh -u -s"
    return 1
    else
    echo "    Verified update: $FILE ($CORRECT_VERSION)"
    fi
    return 0

}

update_file_version()
{
    FILE=$1
    LINE_PATTERN=$2
    NEW_LINE=$3

    echo "Update $FILE [$LINE_PATTERN] => [$NEW_LINE]"
    sed -i s/"$LINE_PATTERN"/"$NEW_LINE"/g $FILE
    return $?
}

update_twoline_version()
{
    FILE=$1
    LINE1_PATTERN=$2
    LINE2_PATTERN=$3
    NEW_LINE1=$4
    NEW_LINE2=$5

    echo "Update $FILE [$LINE1_PATTERN, $LINE2_PATTERN] => [$NEW_LINE1, $NEW_LINE2]"
    sed -i "/$LINE1_PATTERN/ {n;/$LINE2_PATTERN/ {s/$LINE2_PATTERN/$NEW_LINE2/;p;d;}}" $FILE
    return $?
}

BASEDIR=$(dirname "$0")

CHECK=1
SAME=0
UPDATE=0
MAJOR=0
MINOR=0
PATCH=1


while getopts umnpcs option
do
case "${option}" in
u) UPDATE=1
   CHECK=0;;
m) MAJOR=1;;
n) MINOR=1;;
p) PATCH=1;;
c) CHECK=1
   UPDATE=0;;
s) SAME=1;;
*)
    echo "Usage: version.sh [-u] [-c]"
    echo "    -c checks repo versions to ensure they are set properly"
    echo "    -u increments versions depending on option chosen"
    echo "    -s if -c is specified, skips requirement of changed version.txt."
    echo "       if -u is specified, updates all files to reflect version.txt."
    echo "    -m increments major version and sets minor and patch to 0"
    echo "    -n increments minor version and sets patch to 0"
    echo "    -p increments patch version"
    displayTpUsageStatement
    return
    ;;
esac
done

if [ "$CHECK" == "1" ]; then
    VERSION=$(cat $BASEDIR/version.txt)

    if [ "$SAME" != "1" ]; then
        echo "Verify that $BASEDIR/version.txt changed from main"
        git fetch origin main > /dev/null 2>&1
        if [ "$( git diff origin/main -- $BASEDIR/version.txt | wc -l | grep -v 0 )" == "" ]; then
        echo "    Needs update: version.txt"
        echo "       For non-breaking, minor changes (including bugs), run: version.sh -u -p"
        echo "       For non-breaking, new features, run: version.sh -u -n"
        echo "       For major breaking changes, run: version.sh -u -m"
        exit 1
        else
        echo "    Verified update: version.txt"
        fi
    fi

    echo "Check $BASEDIR/version.txt is MAJOR.MINOR.PATCH"
    if [ "$( echo $VERSION | awk -F'.' '{ print NF }' | grep 3  )" == "" ]; then
    echo "    Incorrect format: version.txt"
    exit 1
    else
    echo "    Verified format: $BASEDIR/version.txt"
    fi

    CARGO_FILES="$BASEDIR/shared/Cargo.toml $BASEDIR/agent/Cargo.toml $BASEDIR/controller/Cargo.toml $BASEDIR/samples/brokers/udev-video-broker/Cargo.toml $BASEDIR/webhooks/validating/configuration/Cargo.toml $BASEDIR/discovery-utils/Cargo.toml $BASEDIR/discovery-handlers/debug-echo/Cargo.toml $BASEDIR/discovery-handlers/onvif/Cargo.toml $BASEDIR/discovery-handlers/opcua/Cargo.toml $BASEDIR/discovery-handlers/udev/Cargo.toml $BASEDIR/discovery-handler-modules/debug-echo-discovery-handler/Cargo.toml $BASEDIR/discovery-handler-modules/onvif-discovery-handler/Cargo.toml $BASEDIR/discovery-handler-modules/opcua-discovery-handler/Cargo.toml $BASEDIR/discovery-handler-modules/udev-discovery-handler/Cargo.toml"
    TOML_VERSION_PATTERN="^version"
    TOML_VERSION="\"$(echo $VERSION)\""
    for CARGO_FILE in $CARGO_FILES
    do
        check_file_version "$CARGO_FILE" "$TOML_VERSION_PATTERN" "$TOML_VERSION"
        if [ "$?" -eq "1" ]; then exit 1; fi
    done

    CARGO_LOCK_PROJECTS="controller akri-shared agent controller webhook-configuration udev-video-broker akri-discovery-utils akri-debug-echo akri-udev akri-onvif akri-opcua debug-echo-discovery-handler onvif-discovery-handler udev-discovery-handler opcua-discovery-handler"
    CARGO_LOCK_VERSION="\"$(echo $VERSION)\""
    for CARGO_LOCK_PROJECT in $CARGO_LOCK_PROJECTS
    do
        check_twoline_version "$BASEDIR/Cargo.lock" "name = \"$CARGO_LOCK_PROJECT\"" "$CARGO_LOCK_VERSION"
        if [ "$?" -eq "1" ]; then exit 1; fi
    done

    check_file_version "$BASEDIR/shared/src/akri/mod.rs" "^pub const API_VERSION" "$CRD_VERSION"
    if [ "$?" -eq "1" ]; then exit 1; fi

    CRD_FILES="$BASEDIR/deployment/helm/crds/akri-configuration-crd.yaml $BASEDIR/deployment/helm/crds/akri-instance-crd.yaml"
    CRD_VERSION_PATTERN="^    - name: "
    for CRD_FILE in $CRD_FILES
    do
        check_file_version "$CRD_FILE" "$CRD_VERSION_PATTERN" "$CRD_VERSION"
        if [ "$?" -eq "1" ]; then exit 1; fi
    done

    HELM_VALUES="$BASEDIR/deployment/helm/values.yaml"
    check_twoline_version "$HELM_VALUES" "group: akri.sh" "version: $CRD_VERSION"
    if [ "$?" -eq "1" ]; then exit 1; fi

    HELM_FILES="$BASEDIR/deployment/helm/Chart.yaml"
    for HELM_FILE in $HELM_FILES
    do
        check_file_version "$HELM_FILE" "^version: " "$VERSION"
        if [ "$?" -eq "1" ]; then exit 1; fi

        check_file_version "$HELM_FILE" "^appVersion: " "$VERSION"
        if [ "$?" -eq "1" ]; then exit 1; fi
    done

elif [ "$UPDATE" == "1" ]
then
    OLD_VERSION=$(cat $BASEDIR/version.txt)
    
    if [ "$SAME" != "1" ]; then
        if [ "$MAJOR" == "1" ]; then
            NEW_VERSION="$( echo $OLD_VERSION | awk -F '.' '{print $1 + 1}' ).0.0"
        elif [ "$MINOR" == "1" ]; then
            NEW_VERSION="$( echo $OLD_VERSION | awk -F '.' '{print $1}' ).$( echo $OLD_VERSION | awk -F '.' '{print $2 + 1}' ).0"
        elif [ "$PATCH" == "1" ]; then
            NEW_VERSION="$( echo $OLD_VERSION | awk -F '.' '{print $1}' ).$( echo $OLD_VERSION | awk -F '.' '{print $2}' ).$( echo $OLD_VERSION | awk -F '.' '{print $3 + 1}' )"
        fi
    else
        NEW_VERSION=$(cat $BASEDIR/version.txt)
    fi
    echo "Updating to version: $NEW_VERSION"

    CARGO_FILES="$BASEDIR/shared/Cargo.toml $BASEDIR/agent/Cargo.toml $BASEDIR/controller/Cargo.toml $BASEDIR/samples/brokers/udev-video-broker/Cargo.toml $BASEDIR/webhooks/validating/configuration/Cargo.toml $BASEDIR/discovery-utils/Cargo.toml $BASEDIR/discovery-handlers/debug-echo/Cargo.toml $BASEDIR/discovery-handlers/onvif/Cargo.toml $BASEDIR/discovery-handlers/opcua/Cargo.toml $BASEDIR/discovery-handlers/udev/Cargo.toml $BASEDIR/discovery-handler-modules/debug-echo-discovery-handler/Cargo.toml $BASEDIR/discovery-handler-modules/onvif-discovery-handler/Cargo.toml $BASEDIR/discovery-handler-modules/opcua-discovery-handler/Cargo.toml $BASEDIR/discovery-handler-modules/udev-discovery-handler/Cargo.toml"
    TOML_VERSION_PATTERN="^version = .*"
    TOML_VERSION_LINE="version = \"$NEW_VERSION\""
    for CARGO_FILE in $CARGO_FILES
    do
        update_file_version "$CARGO_FILE" "$TOML_VERSION_PATTERN" "$TOML_VERSION_LINE"
        if [ "$?" -eq "1" ]; then exit 1; fi
    done

    CARGO_LOCK_PROJECTS="controller akri-shared agent controller webhook-configuration udev-video-broker akri-discovery-utils akri-debug-echo akri-udev akri-onvif akri-opcua debug-echo-discovery-handler onvif-discovery-handler udev-discovery-handler opcua-discovery-handler"
    CARGO_LOCK_VERSION_PATTERN="^version = .*"
    CARGO_LOCK_VERSION_LINE="version = \"$NEW_VERSION\""
    for CARGO_LOCK_PROJECT in $CARGO_LOCK_PROJECTS
    do
        update_twoline_version "$BASEDIR/Cargo.lock" "name = \"$CARGO_LOCK_PROJECT\"" "$CARGO_LOCK_VERSION_PATTERN" "name = \"$CARGO_LOCK_PROJECT\"" "$CARGO_LOCK_VERSION_LINE"
        if [ "$?" -eq "1" ]; then exit 1; fi
    done

    CRD_VERSION="v$(echo $NEW_VERSION | awk -F '.' '{print $1}')"

    RS_AKRI_VERSION_PATTERN="^pub const API_VERSION: &str.*"
    RS_AKRI_VERSION_LINE="pub const API_VERSION: \&str = \"$CRD_VERSION\";"
    update_file_version "$BASEDIR/shared/src/akri/mod.rs" "$RS_AKRI_VERSION_PATTERN" "$RS_AKRI_VERSION_LINE"
    if [ "$?" -eq "1" ]; then exit 1; fi

    CRD_FILES="$BASEDIR/deployment/helm/crds/akri-configuration-crd.yaml $BASEDIR/deployment/helm/crds/akri-instance-crd.yaml"
    CRD_VERSION_PATTERN="^    - name: .*"
    CRD_VERSION_LINE="    - name: $CRD_VERSION"
    for CRD_FILE in $CRD_FILES
    do
        update_file_version "$CRD_FILE" "$CRD_VERSION_PATTERN" "$CRD_VERSION_LINE"
        if [ "$?" -eq "1" ]; then exit 1; fi
    done

    HELM_VALUES="$BASEDIR/deployment/helm/values.yaml"
    HELM_VERSION_PATTERN="^  version: .*"
    HELM_VERSION_LINE="  version: $CRD_VERSION"
    update_twoline_version "$HELM_VALUES" "group: akri.sh" "$HELM_VERSION_PATTERN" "group: akri.sh" "$HELM_VERSION_LINE"
    if [ "$?" -eq "1" ]; then exit 1; fi

    HELM_FILES="$BASEDIR/deployment/helm/Chart.yaml"
    for HELM_FILE in $HELM_FILES
    do
        update_file_version "$HELM_FILE" "^version: .*" "version: $NEW_VERSION"
        if [ "$?" -eq "1" ]; then exit 1; fi

        update_file_version "$HELM_FILE" "^appVersion: .*" "appVersion: $NEW_VERSION"
        if [ "$?" -eq "1" ]; then exit 1; fi
    done

    echo $NEW_VERSION > $BASEDIR/version.txt
    echo "Updated to version: $NEW_VERSION"
fi


exit 0
