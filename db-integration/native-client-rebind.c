/* No server connection: synthetic prepared metadata isolates native rebind semantics.
 * MariaDB Connector/C diagnostic only, not a live SQL integration test.
 * The public statement structure is stack-owned and must not be stmt_closed. */
#include <mysql.h>
#include <stdio.h>
#include <string.h>
#include <stddef.h>
int main(void) {
  MYSQL *mysql = mysql_init(NULL);
  if (!mysql) return 2;
  printf("client=%s version=%lu\n", mysql_get_client_info(), mysql_get_client_version());
  printf("MYSQL_BIND size=%zu align=%zu length=%zu is_null=%zu buffer=%zu error=%zu buffer_length=%zu offset=%zu length_value=%zu buffer_type=%zu is_unsigned=%zu extension=%zu\n",
    sizeof(MYSQL_BIND), _Alignof(MYSQL_BIND), offsetof(MYSQL_BIND,length), offsetof(MYSQL_BIND,is_null), offsetof(MYSQL_BIND,buffer), offsetof(MYSQL_BIND,error), offsetof(MYSQL_BIND,buffer_length), offsetof(MYSQL_BIND,offset), offsetof(MYSQL_BIND,length_value), offsetof(MYSQL_BIND,buffer_type), offsetof(MYSQL_BIND,is_unsigned), offsetof(MYSQL_BIND,extension));
  int failed = 0;
  enum enum_field_types types[] = {MYSQL_TYPE_VAR_STRING, MYSQL_TYPE_NEWDECIMAL};
  for (int i=0;i<2;i++) {
    MYSQL_STMT stmt; MYSQL_BIND storage, input; unsigned long length=4;
    char value[]="1.50";
    memset(&stmt,0,sizeof stmt); memset(&storage,0,sizeof storage); memset(&input,0,sizeof input);
    stmt.mysql=mysql; stmt.state=MYSQL_STMT_PREPARED; stmt.field_count=1; stmt.bind=&storage;
    input.buffer_type=types[i]; input.buffer=value; input.buffer_length=4; input.length=&length;
    int rc=mysql_stmt_bind_result(&stmt,&input);
    printf("type=%s rc=%d before=4 after=%lu\n",i?"NEWDECIMAL":"VAR_STRING",rc,length);
    if (rc != 0 || length != 4) failed = 1;
  }
  mysql_close(mysql);
  if (failed) {
    fprintf(stderr, "BLOCKED: native rebind preflight failed; inspect rc and before/after lengths above. A successful bind that resets length is the CONC-821 pattern. Diesel Text/Numeric validation is not established.\n");
    return 1;
  }
  return 0;
}
