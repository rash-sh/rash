#!/bin/bash
# requirements:
#   - pv
#   - terminalizer

set -e

TMPFILE_PATH=$(mktemp)
# shellcheck disable=SC2164
LOCAL_DIR="$(cd "$(dirname "$0")" ; pwd -P)"
COMMANDS='#!/bin/bash
function cat () {
  pygmentize -l yaml+jinja -O full,style=monokai "$@"
}
'

trap clean_up EXIT

clean_up()
{
    echo "Cleaning up..."
    rm -rf "$TMPFILE_PATH*"
}

build_terminalizer_text_from_examples()
{
  while IFS= read -r -d '' FILE; do
    for COMMAND in "cat $FILE" "$FILE"; do
      COMMANDS+="\n\
echo \$ $COMMAND | pv -qL $((7+(-2 + RANDOM%5))) \n\
$COMMAND \n\
sleep 4 \n\
"
    done
    COMMANDS+="\n\
sleep 4 \n\
clear \n\
"
  done <   <(find "examples" -type f -name '*.rh' -print0)

  COMMANDS+="sleep 4;echo 'try it! :)' | pv -qL $((7));sleep 4"

  echo -e "$COMMANDS" > "$TMPFILE_PATH"
  chmod +x "$TMPFILE_PATH"
}

build_terminalizer_text()
{
  COMMANDS+="\n\
clear \n\
echo cd examples/envar-api-gateway | pv -qL $((7+(-2 + RANDOM%5))) \n\
cd examples/envar-api-gateway \n\
echo cat Dockerfile | pv -qL $((7+(-2 + RANDOM%5))) \n\
cat Dockerfile \n\
sleep 4 \n\
echo cat entrypoint.rh | pv -qL $((7+(-2 + RANDOM%5))) \n\
cat entrypoint.rh \n\
sleep 8 \n\
echo docker build -t envar-api-gateway . | pv -qL $((7+(-2 + RANDOM%5))) \n\
docker build -t envar-api-gateway . \n\
echo docker run -e DOMAINS=rash.sh,buildpacks.io '\\ \n'-p 80:80 envar-api-gateway \& | pv -qL $((10+(-2 + RANDOM%5))) \n\
docker run -e DOMAINS=rash.sh,buildpacks.io -p 80:80 --rm envar-api-gateway & \n\
sleep 6 \n\
clear \n\
echo curl -so /dev/null 127.0.0.1/rash | pv -qL $((7+(-2 + RANDOM%5))) \n\
curl -so /dev/null 127.0.0.1/rash -w 'http_code: %{http_code}\nlocation:  %{redirect_url}\n' \n\
sleep 4 \n\
clear \n\
echo curl -so /dev/null 127.0.0.1/buildpacks | pv -qL $((10+(-2 + RANDOM%5))) \n\
curl -so /dev/null 127.0.0.1/buildpacks -w 'http_code: %{http_code}\nlocation:  %{redirect_url}\n' \n\
sleep 4 \n\
clear \n\
echo try it! | pv -qL $((7+(-2 + RANDOM%5))) \n\
sleep 2 \n\
clear \n\
"
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

TERMINALIZE_FILE=$(mktemp).yml
GIF_FILE=${TERMINALIZE_FILE}.gif
terminalizer record -k -c "${CONFIG_PATH}" ${TERMINALIZE_FILE}
terminalizer render ${TERMINALIZE_FILE} -q 100 -o ${GIF_FILE}

ffmpeg -i ${GIF_FILE} -y -movflags faststart -pix_fmt yuv420p -vf "scale=trunc(iw/2)*2:trunc(ih/2)*2" "$1"
