<map version="0.9.0">
  <node ID="ID_1589306110562" TEXT="rash" COLOR="#000000">
    <font SIZE="12" BOLD="true" ITALIC="false"/>
    <node ID="ID_1589306110563" TEXT="data" COLOR="#000000" POSITION="left">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <node ID="ID_1589306110564" TEXT="Context" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <node ID="ID_1589306110565" TEXT="Facts" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
          <richcontent TYPE="NOTE">
            <html>
              <body>
                <p><b>Todos:</b><br/>env - Normal priority - 100% - 05/11/2020</p>
                <br/>
              </body>
            </html>
          </richcontent>
        </node>
        <node ID="ID_1589306110566" TEXT="Tasks" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
          <node ID="ID_1589306110567" TEXT="Task" COLOR="#000000">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
            <richcontent TYPE="NOTE">
              <html>
                <body>
                  <p><b>Todos:</b><br/>TaskNew - Normal priority - 100% - 05/11/2020<br/>TaskValid - Normal priority - 100% - 05/11/2020<br/>Task - Normal priority - 100% - 05/11/2020</p>
                  <br/>
                </body>
              </html>
            </richcontent>
            <node ID="ID_1589306110568" TEXT="name" COLOR="#000000">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
              <icon BUILTIN="help"/>
            </node>
            <node ID="ID_1589306110569" TEXT="params" COLOR="#000000">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
              <richcontent TYPE="NOTE">
                <html>
                  <body>
                    <div>flexible to satisfy all modules</div>
                  </body>
                </html>
              </richcontent>
            </node>
            <node ID="ID_1589306110570" TEXT="Module" COLOR="#000000">
              <font SIZE="12" BOLD="false" ITALIC="false"/>
              <richcontent TYPE="NOTE">
                <html>
                  <body>
                    <p><b>Todos:</b><br/>command - Normal priority - 50% - 05/11/2020<br/>request - Normal priority - 0% - 05/11/2020<br/>copy - Normal priority - 0% - 05/11/2020<br/>template - Normal priority - 0% - 05/11/2020<br/>lineinfile - Normal priority - 0% - 05/11/2020</p>
                    <br/>
                  </body>
                </html>
              </richcontent>
              <node ID="ID_1589306110571" TEXT="name" COLOR="#000000">
                <font SIZE="12" BOLD="false" ITALIC="false"/>
              </node>
              <node ID="ID_1589306110572" TEXT="exec_fn" COLOR="#000000">
                <font SIZE="12" BOLD="false" ITALIC="false"/>
              </node>
            </node>
          </node>
        </node>
      </node>
      <node ID="ID_1589306110573" TEXT="ModuleResult" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <node ID="ID_1589306110574" TEXT="changed" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
        </node>
        <node ID="ID_1589306110575" TEXT="extra" COLOR="#000000">
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
    <node ID="ID_1589306110576" TEXT="error" COLOR="#000000" POSITION="left">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <icon BUILTIN="messagebox_warning"/>
      <node ID="ID_1589306110577" TEXT="repr" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <node ID="ID_1589306110578" TEXT="Simple" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
        </node>
        <node ID="ID_1589306110579" TEXT="Custom" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
          <node ID="ID_1589306110580" TEXT="ErrorKind" COLOR="#000000">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
          </node>
          <node ID="ID_1589306110581" TEXT="Error" COLOR="#000000">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
          </node>
        </node>
      </node>
    </node>
    <node ID="ID_1589306110582" TEXT="log" COLOR="#000000" POSITION="right">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <icon BUILTIN="info"/>
      <node ID="ID_1589306110583" TEXT="trace" COLOR="#3fbaee">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <icon BUILTIN="flag-blue"/>
        <richcontent TYPE="NOTE">
          <html>
            <body>
              <div>trace modules execution (command, request, copy...)</div>
              <div>argument: -vv<br/></div>
            </body>
          </html>
        </richcontent>
      </node>
      <node ID="ID_1589306110584" TEXT="debug" COLOR="#3fbaee">
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
      <node ID="ID_1589306110585" TEXT="info" COLOR="#8ac25b">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <node ID="ID_1589306110586" TEXT="task" COLOR="#8ac25b">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
          <node ID="ID_1589306110587" TEXT="separator" COLOR="#777777">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
          </node>
          <node ID="ID_1589306110588" TEXT="changed" COLOR="#fea852">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
            <icon BUILTIN="flag-yellow"/>
          </node>
          <node ID="ID_1589306110589" TEXT="ok" COLOR="#8ac25b">
            <font SIZE="12" BOLD="false" ITALIC="false"/>
            <icon BUILTIN="flag-green"/>
          </node>
        </node>
      </node>
      <node ID="ID_1589306110590" TEXT="warning" COLOR="#8971c1">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <icon BUILTIN="flag-pink"/>
      </node>
      <node ID="ID_1589306110591" TEXT="error" COLOR="#fc6e6e">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <icon BUILTIN="flag"/>
      </node>
    </node>
    <node ID="ID_1589306110592" TEXT="execution" COLOR="#000000" POSITION="right">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <icon BUILTIN="list"/>
      <node ID="ID_1589306110593" TEXT="context.exec" COLOR="#000000">
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
        <node ID="ID_1589306110594" TEXT="task.exec" COLOR="#000000">
          <font SIZE="12" BOLD="false" ITALIC="false"/>
          <richcontent TYPE="NOTE">
            <html>
              <body>
                <div>exec(module, parameters, facts) -&gt; module_result</div>
              </body>
            </html>
          </richcontent>
          <node ID="ID_1589306110595" TEXT="module.exec" COLOR="#000000">
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
      <node ID="ID_1589306110596" TEXT="read_file" COLOR="#000000">
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
    <node ID="ID_1589306110597" TEXT="input" COLOR="#000000" POSITION="right">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <icon BUILTIN="male1"/>
      <node ID="ID_1589306110598" TEXT="read_file" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
      </node>
    </node>
    <node ID="ID_1589306110599" TEXT="release" COLOR="#000000" POSITION="right">
      <font SIZE="12" BOLD="true" ITALIC="false"/>
      <node ID="ID_1589306110600" TEXT="Dockerfile" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
        <richcontent TYPE="NOTE">
          <html>
            <body>
              <div>`N` flavours, compile from args (envars) or target file (read modules and compile just necessary ones)</div>
            </body>
          </html>
        </richcontent>
      </node>
      <node ID="ID_1589306110601" TEXT="binaries" COLOR="#000000">
        <font SIZE="12" BOLD="false" ITALIC="false"/>
      </node>
    </node>
  </node>
</map>
