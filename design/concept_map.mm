<map version="0.9.0">
  <node ID="ID_1589647293130" TEXT="rash" COLOR="#000000">
    <font SIZE="12" BOLD="true" ITALIC="false"/>
    <node ID="ID_1589647293131" TEXT="data" COLOR="#000000" POSITION="left">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <node ID="ID_1589647293132" TEXT="Context" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <node ID="ID_1589647293133" TEXT="Facts" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
          <richcontent TYPE="NOTE">
            <html>
              <body>
                <p><b>Todos:</b><br/>env - Normal priority - 100% - 05/11/2020<br/>cli - Normal priority - 0% - 05/15/2020</p>
                <br/>
              </body>
            </html>
          </richcontent>
        </node>
        <node ID="ID_1589647293134" TEXT="Tasks" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
          <node ID="ID_1589647293135" TEXT="Task" COLOR="#000000">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
            <richcontent TYPE="NOTE">
              <html>
                <body>
                  <p><b>Todos:</b><br/>TaskNew - Normal priority - 100% - 05/11/2020<br/>TaskValid - Normal priority - 100% - 05/11/2020<br/>Task - Normal priority - 100% - 05/11/2020</p>
                  <br/>
                </body>
              </html>
            </richcontent>
            <node ID="ID_1589647293136" TEXT="name" COLOR="#000000">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
              <icon BUILTIN="help"/>
            </node>
            <node ID="ID_1589647293137" TEXT="params" COLOR="#000000">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
              <richcontent TYPE="NOTE">
                <html>
                  <body>
                    <div>flexible to satisfy all modules</div>
                  </body>
                </html>
              </richcontent>
            </node>
            <node ID="ID_1589647293138" TEXT="Module" COLOR="#000000">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
              <richcontent TYPE="NOTE">
                <html>
                  <body>
                    <p><b>Todos:</b><br/>command - Normal priority - 50% - 05/11/2020<br/>request - Normal priority - 0% - 05/11/2020<br/>copy - Normal priority - 0% - 05/11/2020<br/>template - Normal priority - 0% - 05/11/2020<br/>lineinfile - Normal priority - 0% - 05/11/2020</p>
                    <br/>
                  </body>
                </html>
              </richcontent>
              <node ID="ID_1589647293139" TEXT="name" COLOR="#000000">
                <font SIZE="12" BOLD="false" ITALIC="false"/>
              </node>
              <node ID="ID_1589647293140" TEXT="exec_fn" COLOR="#000000">
                <font SIZE="12" BOLD="false" ITALIC="false"/>
              </node>
            </node>
            <node ID="ID_1589647293141" TEXT="when" COLOR="#000000">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
              <icon BUILTIN="help"/>
            </node>
            <node ID="ID_1589647293142" TEXT="loop" COLOR="#000000">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
              <icon BUILTIN="help"/>
            </node>
            <node ID="ID_1589647293143" TEXT="changed_when" COLOR="#000000">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
              <icon BUILTIN="help"/>
            </node>
            <node ID="ID_1589647293144" TEXT="ignore_errors" COLOR="#000000">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
              <icon BUILTIN="help"/>
            </node>
          </node>
        </node>
      </node>
      <node ID="ID_1589647293145" TEXT="ModuleResult" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <node ID="ID_1589647293146" TEXT="changed" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
        </node>
        <node ID="ID_1589647293147" TEXT="extra" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
          <icon BUILTIN="help"/>
          <richcontent TYPE="NOTE">
            <html>
              <body>
                <div>flexible to satisfy all modules</div>
              </body>
            </html>
          </richcontent>
        </node>
      </node>
      <node ID="ID_1589647293148" TEXT="Block" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <richcontent TYPE="NOTE">
          <html>
            <body>
              <div>group tasks with similar fields (when, changed_when, ignore_errors)</div>
            </body>
          </html>
        </richcontent>
        <node ID="ID_1589647293149" TEXT="Tasks" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
        </node>
      </node>
    </node>
    <node ID="ID_1589647293150" TEXT="error" COLOR="#000000" POSITION="left">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <icon BUILTIN="messagebox_warning"/>
      <node ID="ID_1589647293151" TEXT="repr" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <node ID="ID_1589647293152" TEXT="Simple" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
        </node>
        <node ID="ID_1589647293153" TEXT="Custom" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
          <node ID="ID_1589647293154" TEXT="ErrorKind" COLOR="#000000">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
          </node>
          <node ID="ID_1589647293155" TEXT="Error" COLOR="#000000">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
          </node>
        </node>
      </node>
    </node>
    <node ID="ID_1589647293156" TEXT="log" COLOR="#000000" POSITION="left">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <icon BUILTIN="info"/>
      <node ID="ID_1589647293157" TEXT="trace" COLOR="#777777">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <richcontent TYPE="NOTE">
          <html>
            <body>
              <div>trace modules execution (command, request, copy...)</div>
              <div>argument: -vv<br/></div>
            </body>
          </html>
        </richcontent>
        <node ID="ID_1589647293158" TEXT="trace" COLOR="#777777">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
        </node>
        <node ID="ID_1589647293159" TEXT="error" COLOR="#fc6e6e">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
        </node>
      </node>
      <node ID="ID_1589647293160" TEXT="debug" COLOR="#3fbaee">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <icon BUILTIN="flag-blue"/>
        <richcontent TYPE="NOTE">
          <html>
            <body>
              <div>traces main modules ( facts, context, tasks...)</div>
              <div>argument: -v<br/></div>
            </body>
          </html>
        </richcontent>
      </node>
      <node ID="ID_1589647293161" TEXT="info" COLOR="#8ac25b">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <node ID="ID_1589647293162" TEXT="task" COLOR="#8ac25b">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
          <node ID="ID_1589647293163" TEXT="separator" COLOR="#777777">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
            <node ID="ID_1589647293164" TEXT="tasks to go" COLOR="#777777">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
            </node>
            <node ID="ID_1589647293165" TEXT="task name" COLOR="#777777">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
            </node>
          </node>
          <node ID="ID_1589647293166" TEXT="changed" COLOR="#fea852">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
            <icon BUILTIN="flag-yellow"/>
          </node>
          <node ID="ID_1589647293167" TEXT="ok" COLOR="#8ac25b">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
            <icon BUILTIN="flag-green"/>
          </node>
        </node>
      </node>
      <node ID="ID_1589647293168" TEXT="warning" COLOR="#8971c1">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <icon BUILTIN="flag-pink"/>
      </node>
      <node ID="ID_1589647293169" TEXT="error" COLOR="#fc6e6e">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <icon BUILTIN="flag"/>
      </node>
    </node>
    <node ID="ID_1589647293170" TEXT="execution" COLOR="#000000" POSITION="left">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <icon BUILTIN="list"/>
      <node ID="ID_1589647293171" TEXT="context.exec" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <richcontent TYPE="NOTE">
          <html>
            <body>
              <div>exec(task[0], facts_0) -&gt; facts_1</div>
              <div>exec(task[1], facts_1) -&gt; facts_2</div>
              <div>...</div>
              <div>
                <br/>
              </div>
            </body>
          </html>
        </richcontent>
        <node ID="ID_1589647293172" TEXT="task.exec" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
          <richcontent TYPE="NOTE">
            <html>
              <body>
                <div>exec(module, parameters, facts) -&gt; module_result</div>
              </body>
            </html>
          </richcontent>
          <node ID="ID_1589647293173" TEXT="module.exec" COLOR="#000000">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
            <richcontent TYPE="NOTE">
              <html>
                <body>
                  <p>exec_fn(rendered_params) -&gt; module_result</p>
                </body>
              </html>
            </richcontent>
          </node>
        </node>
      </node>
      <node ID="ID_1589647293174" TEXT="read_file" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <richcontent TYPE="NOTE">
          <html>
            <body>
              <div>get tasks from file</div>
            </body>
          </html>
        </richcontent>
      </node>
    </node>
    <node ID="ID_1589647293175" TEXT="plugins" COLOR="#000000" POSITION="right">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <icon BUILTIN="bell"/>
      <node ID="ID_1589647293176" TEXT="lookup" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <node ID="ID_1589647293177" TEXT="etcd" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
        </node>
        <node ID="ID_1589647293178" TEXT="vault" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
        </node>
        <node ID="ID_1589647293179" TEXT="s3" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
        </node>
      </node>
      <node ID="ID_1589647293180" TEXT="filter" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
      </node>
    </node>
    <node ID="ID_1589647293181" TEXT="release" COLOR="#000000" POSITION="right">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <node ID="ID_1589647293182" TEXT="Dockerfile" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <richcontent TYPE="NOTE">
          <html>
            <body>
              <div>`N` flavours, compile from args (envars) or target file (read modules and compile just necessary ones)</div>
            </body>
          </html>
        </richcontent>
      </node>
      <node ID="ID_1589647293183" TEXT="binaries" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
      </node>
    </node>
    <node ID="ID_1589647293184" TEXT="cli" COLOR="#000000" POSITION="right">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <icon BUILTIN="male1"/>
      <node ID="ID_1589647293185" TEXT="envars" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <richcontent TYPE="NOTE">
          <html>
            <body>
              <p>-e KEY=VALUE to use as {{ env.KEY }}</p>
            </body>
          </html>
        </richcontent>
      </node>
      <node ID="ID_1589647293186" TEXT="verbosity" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
      </node>
      <node ID="ID_1589647293187" TEXT="script-file" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
      </node>
    </node>
    <node ID="ID_1589647293188" TEXT="integrated documentation" COLOR="#000000" POSITION="right">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <icon BUILTIN="list"/>
    </node>
    <node ID="ID_1589647293189" TEXT="integrated testing tool" COLOR="#000000" POSITION="right">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
    </node>
    <node ID="ID_1589647293190" TEXT="Repository" COLOR="#000000" POSITION="right">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <icon BUILTIN="idea"/>
      <richcontent TYPE="NOTE">
        <html>
          <body>
            <p>Repository of entrypoints.rh or custom scripts</p>
          </body>
        </html>
      </richcontent>
    </node>
  </node>
</map>
