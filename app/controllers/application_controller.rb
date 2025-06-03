class ApplicationController < ActionController::API
    include ActionController::HttpAuthentication::Token::ControllerMethods

    before_action :authenticate_api_key

    attr_reader :current_api_key

    protected

    def authenticate_api_key
        authenticate_or_request_with_http_token do |token, options|
            @current_api_key = ApiKey.find_by_token(token)
            @current_api_key.present?
        end
    end

    def request_http_token_authentication(realm = "Application", message = nil)
        json_response = { success: false, message: message || "Access denied" }
        headers["WWW-Authenticate"] = %(Bearer realm="#{realm.tr('"', "")}")
        render json: json_response, status: :unauthorized
    end
end
