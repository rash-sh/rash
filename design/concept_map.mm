<map version="0.9.0">
  <node ID="ID_1589551164854" TEXT="rash" COLOR="#000000">
    <font SIZE="12" BOLD="true" ITALIC="false"/>
    <node ID="ID_1589551164855" TEXT="data" COLOR="#000000" POSITION="left">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <node ID="ID_1589551164856" TEXT="Context" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <node ID="ID_1589551164857" TEXT="Facts" COLOR="#000000">
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
        <node ID="ID_1589551164858" TEXT="Tasks" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
          <node ID="ID_1589551164859" TEXT="Task" COLOR="#000000">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
            <richcontent TYPE="NOTE">
              <html>
                <body>
                  <p><b>Todos:</b><br/>TaskNew - Normal priority - 100% - 05/11/2020<br/>TaskValid - Normal priority - 100% - 05/11/2020<br/>Task - Normal priority - 100% - 05/11/2020</p>
                  <br/>
                </body>
              </html>
            </richcontent>
            <node ID="ID_1589551164860" TEXT="name" COLOR="#000000">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
              <icon BUILTIN="help"/>
            </node>
            <node ID="ID_1589551164861" TEXT="params" COLOR="#000000">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
              <richcontent TYPE="NOTE">
                <html>
                  <body>
                    <div>flexible to satisfy all modules</div>
                  </body>
                </html>
              </richcontent>
            </node>
            <node ID="ID_1589551164862" TEXT="Module" COLOR="#000000">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
              <richcontent TYPE="NOTE">
                <html>
                  <body>
                    <p><b>Todos:</b><br/>command - Normal priority - 50% - 05/11/2020<br/>request - Normal priority - 0% - 05/11/2020<br/>copy - Normal priority - 0% - 05/11/2020<br/>template - Normal priority - 0% - 05/11/2020<br/>lineinfile - Normal priority - 0% - 05/11/2020</p>
                    <br/>
                  </body>
                </html>
              </richcontent>
              <node ID="ID_1589551164863" TEXT="name" COLOR="#000000">
                <font SIZE="12" BOLD="false" ITALIC="false"/>
              </node>
              <node ID="ID_1589551164864" TEXT="exec_fn" COLOR="#000000">
                <font SIZE="12" BOLD="false" ITALIC="false"/>
              </node>
            </node>
            <node ID="ID_1589551164865" TEXT="when" COLOR="#000000">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
              <icon BUILTIN="help"/>
            </node>
            <node ID="ID_1589551164866" TEXT="loop" COLOR="#000000">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
              <icon BUILTIN="help"/>
            </node>
            <node ID="ID_1589551164867" TEXT="changed_when" COLOR="#000000">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
              <icon BUILTIN="help"/>
            </node>
            <node ID="ID_1589551164868" TEXT="ignore_errors" COLOR="#000000">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
              <icon BUILTIN="help"/>
            </node>
          </node>
        </node>
      </node>
      <node ID="ID_1589551164869" TEXT="ModuleResult" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <node ID="ID_1589551164870" TEXT="changed" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
        </node>
        <node ID="ID_1589551164871" TEXT="extra" COLOR="#000000">
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
    </node>
    <node ID="ID_1589551164872" TEXT="error" COLOR="#000000" POSITION="left">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <icon BUILTIN="messagebox_warning"/>
      <node ID="ID_1589551164873" TEXT="repr" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <node ID="ID_1589551164874" TEXT="Simple" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
        </node>
        <node ID="ID_1589551164875" TEXT="Custom" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
          <node ID="ID_1589551164876" TEXT="ErrorKind" COLOR="#000000">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
          </node>
          <node ID="ID_1589551164877" TEXT="Error" COLOR="#000000">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
          </node>
        </node>
      </node>
    </node>
    <node ID="ID_1589551164878" TEXT="log" COLOR="#000000" POSITION="left">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <icon BUILTIN="info"/>
      <node ID="ID_1589551164879" TEXT="trace" COLOR="#777777">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <richcontent TYPE="NOTE">
          <html>
            <body>
              <div>trace modules execution (command, request, copy...)</div>
              <div>argument: -vv<br/></div>
            </body>
          </html>
        </richcontent>
        <node ID="ID_1589551164880" TEXT="trace" COLOR="#777777">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
        </node>
        <node ID="ID_1589551164881" TEXT="error" COLOR="#fc6e6e">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
        </node>
      </node>
      <node ID="ID_1589551164882" TEXT="debug" COLOR="#3fbaee">
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
      <node ID="ID_1589551164883" TEXT="info" COLOR="#8ac25b">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <node ID="ID_1589551164884" TEXT="task" COLOR="#8ac25b">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
          <node ID="ID_1589551164885" TEXT="separator" COLOR="#777777">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
            <node ID="ID_1589551164886" TEXT="tasks to go" COLOR="#777777">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
            </node>
            <node ID="ID_1589551164887" TEXT="task name" COLOR="#777777">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
            </node>
          </node>
          <node ID="ID_1589551164888" TEXT="changed" COLOR="#fea852">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
            <icon BUILTIN="flag-yellow"/>
          </node>
          <node ID="ID_1589551164889" TEXT="ok" COLOR="#8ac25b">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
            <icon BUILTIN="flag-green"/>
          </node>
        </node>
      </node>
      <node ID="ID_1589551164890" TEXT="warning" COLOR="#8971c1">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <icon BUILTIN="flag-pink"/>
      </node>
      <node ID="ID_1589551164891" TEXT="error" COLOR="#fc6e6e">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <icon BUILTIN="flag"/>
      </node>
    </node>
    <node ID="ID_1589551164892" TEXT="execution" COLOR="#000000" POSITION="left">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <icon BUILTIN="list"/>
      <node ID="ID_1589551164893" TEXT="context.exec" COLOR="#000000">
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
        <node ID="ID_1589551164894" TEXT="task.exec" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
          <richcontent TYPE="NOTE">
            <html>
              <body>
                <div>exec(module, parameters, facts) -&gt; module_result</div>
              </body>
            </html>
          </richcontent>
          <node ID="ID_1589551164895" TEXT="module.exec" COLOR="#000000">
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
      <node ID="ID_1589551164896" TEXT="read_file" COLOR="#000000">
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
    <node ID="ID_1589551164897" TEXT="plugins" COLOR="#000000" POSITION="right">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <icon BUILTIN="bell"/>
      <node ID="ID_1589551164898" TEXT="lookup" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <node ID="ID_1589551164899" TEXT="etcd" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
        </node>
        <node ID="ID_1589551164900" TEXT="vault" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
        </node>
        <node ID="ID_1589551164901" TEXT="s3" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
        </node>
      </node>
      <node ID="ID_1589551164902" TEXT="filter" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
      </node>
    </node>
    <node ID="ID_1589551164903" TEXT="release" COLOR="#000000" POSITION="right">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <node ID="ID_1589551164904" TEXT="Dockerfile" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <richcontent TYPE="NOTE">
          <html>
            <body>
              <div>`N` flavours, compile from args (envars) or target file (read modules and compile just necessary ones)</div>
            </body>
          </html>
        </richcontent>
      </node>
      <node ID="ID_1589551164905" TEXT="binaries" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
      </node>
    </node>
    <node ID="ID_1589551164906" TEXT="cli" COLOR="#000000" POSITION="right">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <icon BUILTIN="male1"/>
      <node ID="ID_1589551164907" TEXT="envars" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <richcontent TYPE="NOTE">
          <html>
            <body>
              <p>-e KEY=VALUE to use as {{ env.KEY }}</p>
            </body>
          </html>
        </richcontent>
      </node>
      <node ID="ID_1589551164908" TEXT="verbosity" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
      </node>
      <node ID="ID_1589551164909" TEXT="script-file" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
      </node>
    </node>
    <node ID="ID_1589551164910" TEXT="integrated documentation" COLOR="#000000" POSITION="right">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <icon BUILTIN="list"/>
    </node>
    <node ID="ID_1589551164911" TEXT="integrated testing tool" COLOR="#000000" POSITION="right">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
    </node>
    <node ID="ID_1589551164912" TEXT="Repository" COLOR="#000000" POSITION="right">
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
