#!/bin/bash
# usage: ./ci/demo.sh [output-file]
# requirements:
#   - pv
#   - terminalizer

set -e

TMPFILE_PATH=$(mktemp)
# shellcheck disable=SC2164
LOCAL_DIR="$(cd "$(dirname "$0")" ; pwd -P)"
COMMANDS='#!/bin/bash
function cat () {
  pygmentize -l yaml+jinja -O full,style=emacs "$@"
}
'

trap clean_up EXIT

clean_up()
{
    echo "Cleaning up..."
    rm -rf "$TMPFILE_PATH*"
}

build_terminalizer_text()
{
  while IFS= read -r -d '' FILE; do
    for COMMAND in "cat $FILE" "$FILE"; do
      COMMANDS+="\n\
echo \$ $COMMAND | pv -qL $((10+(-2 + RANDOM%5))) \n\
$COMMAND \n\
sleep 3 \n\
"
    done
    COMMANDS+="\n\
sleep 3 \n\
clear \n\
"
  done <   <(find "examples" -type f -name '*.rh' -print0)

  COMMANDS+="sleep 2;echo 'try it! :)' | pv -qL $((10));sleep 2"

  echo -e "$COMMANDS" > "$TMPFILE_PATH"
  chmod +x "$TMPFILE_PATH"
}

build_terminalizer_config()
{
CONFIG_PATH=${TMPFILE_PATH}_terminalizer_config.yml
cat <<EOF > "${CONFIG_PATH}"
command: $TMPFILE_PATH
cwd: $(pwd)
env:
  MY_PASSWORD: supersecret
  USER: rash-user
  BROWSER: /usr/bin/firefox
  recording: true
cols: auto
rows: auto
repeat: 0
quality: 100
frameDelay: auto
maxIdleTime: 2000
frameBox:
  type: solid
  title: null
  style:
    boxShadow: none
    margin: 0px
watermark:
  imagePath: null
  style:
    position: absolute
    right: 15px
    bottom: 15px
    width: 100px
    opacity: 0.9

cursorStyle: block
fontFamily: "Monaco, Lucida Console, Ubuntu Mono, Monospace"
fontSize: 12
lineHeight: 1
letterSpacing: 0
theme:
  background: "transparent"

EOF
}

## Main

if [ -z "$1" ]; then
    echo "Usage: $0 <output_file>"
    exit 1
fi

build_terminalizer_text

build_terminalizer_config

TERMINALIZE_FILE=$(mktemp)
GIF_FILE=${TERMINALIZE_FILE}.gif
terminalizer record -k -c "${CONFIG_PATH}" ${TERMINALIZE_FILE}
terminalizer render ${TERMINALIZE_FILE} -q 100 -o ${GIF_FILE}

ffmpeg -i ${GIF_FILE} -c vp9 -b:v 0 -crf 41 "$1"
