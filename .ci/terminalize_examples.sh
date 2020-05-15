#!/bin/bash
# usage: ./ci/demo.sh [output-file]
# requirements:
#   - pv
#   - terminalizer


TMPFILE_PATH=$(mktemp)
COMMANDS="#!/bin/bash"

for FILE in $(find examples -type f); do
    for COMMAND in "cat $FILE" "$FILE"; do
    if [ "$COMMAND" = "example/env_missing.rh" ];
    then
        COMMAND="$COMMAND -vv"
    fi
    COMMANDS+="\n\
echo \$ $COMMAND | pv -qL $[10+(-2 + RANDOM%5)] \n\
$COMMAND \n\
sleep 2 \n\
"
    done
done

echo -e $COMMANDS > $TMPFILE_PATH
chmod +x $TMPFILE_PATH


CONFIG_PATH=${TMPFILE_PATH}_terminalizer_config.yml
cat <<EOF > ${CONFIG_PATH}
command: $TMPFILE_PATH
cwd: $(pwd)
env:
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

terminalizer record -k -c ${CONFIG_PATH} $1
